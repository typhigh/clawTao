/**
 * ClawTao Electron Main Process.
 *
 * Spawns the Rust backend as a child process and communicates with it
 * via JSON-RPC 2.0 over stdin/stdout. The renderer (React) talks to
 * main through standard Electron IPC; main translates and forwards to Rust.
 *
 * Streaming LLM responses from Rust appear as JSON-RPC notifications
 * on stdout, which main converts to IPC events for the renderer.
 *
 * API Key is encrypted at rest via Electron safeStorage (OS-level encryption).
 * Rust never sees the encrypted form — main decrypts and injects the plaintext
 * key into Rust at startup and on config changes.
 */
import { app, BrowserWindow, ipcMain, safeStorage, shell } from 'electron';
import path from 'path';
import { spawn, ChildProcess } from 'child_process';
import * as readline from 'readline';
import * as fs from 'fs';

let mainWindow: BrowserWindow | null = null;
let rustProcess: ChildProcess | null = null;
const pendingRequests = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
let requestId = 0;

const isDev = process.env.NODE_ENV !== 'production' || !app.isPackaged;

// -- secrets management (safeStorage) --

function secretsPath(): string {
  const dataDir = path.join(app.getPath('userData'), 'clawtao');
  if (!fs.existsSync(dataDir)) fs.mkdirSync(dataDir, { recursive: true });
  return path.join(dataDir, 'secrets.json');
}

function readEncryptedKey(): string | null {
  try {
    const data = JSON.parse(fs.readFileSync(secretsPath(), 'utf-8'));
    if (data.api_key && safeStorage.isEncryptionAvailable()) {
      return safeStorage.decryptString(Buffer.from(data.api_key, 'base64'));
    }
  } catch {}
  return null;
}

function writeEncryptedKey(plaintext: string): void {
  if (!safeStorage.isEncryptionAvailable()) {
    console.warn('safeStorage unavailable — storing key as plaintext');
    fs.writeFileSync(secretsPath(), JSON.stringify({ api_key: Buffer.from(plaintext).toString('base64') }));
    return;
  }
  const encrypted = safeStorage.encryptString(plaintext).toString('base64');
  fs.writeFileSync(secretsPath(), JSON.stringify({ api_key: encrypted }));
}

// -- Config persistence (Electron-managed) --

function configPath(): string {
  const dataDir = path.join(app.getPath('userData'), 'clawtao');
  if (!fs.existsSync(dataDir)) fs.mkdirSync(dataDir, { recursive: true });
  return path.join(dataDir, 'config.json');
}

function readConfig(): Record<string, unknown> {
  try {
    return JSON.parse(fs.readFileSync(configPath(), 'utf-8'));
  } catch {
    return {};
  }
}

function writeConfig(cfg: Record<string, unknown>): void {
  fs.writeFileSync(configPath(), JSON.stringify(cfg, null, 2));
}

// -- Browser server --

function startBrowserServer() {
  const script = path.join(__dirname, '../../core/scripts/browser-server.mjs');
  try {
    const proc = spawn('node', [script], { stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env } });
    proc.stdout.on('data', (d: Buffer) => console.log(`[browser] ${d}`.trim()));
    proc.stderr.on('data', (d: Buffer) => console.error(`[browser] ${d}`.trim()));
    proc.on('error', (err) => console.error('Browser server error:', err.message));
    proc.on('exit', (code) => {
      console.log(`Browser server exited: ${code}`);
      if (code !== 0) setTimeout(startBrowserServer, 3000);
    });
    console.log('Browser server starting...');
  } catch (e) {
    console.error('Failed to start browser server:', e);
  }
}

// -- Rust process management --

function startRust() {
  const manifestPath = path.join(__dirname, '../../core/Cargo.toml');
  const coreDir = path.dirname(manifestPath);

  rustProcess = spawn('cargo', ['run', '--manifest-path', manifestPath], {
    cwd: coreDir,
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, RUST_BACKTRACE: '1' },
  });

  const rl = readline.createInterface({ input: rustProcess.stdout!, crlfDelay: Infinity });
  rl.on('line', (line: string) => {
    if (!line.trim()) return;
    try {
      const msg = JSON.parse(line);
      if (msg.id != null && msg.result !== undefined) {
        const p = pendingRequests.get(msg.id);
        if (p) { pendingRequests.delete(msg.id); p.resolve(msg.result); }
      } else if (msg.id != null && msg.error) {
        const p = pendingRequests.get(msg.id);
        if (p) { pendingRequests.delete(msg.id); p.reject(new Error(msg.error.message)); }
      } else if (msg.method) {
        const channel = msg.method.replace(/\./g, ':');
        mainWindow?.webContents.send(channel, msg.params);
      }
    } catch {}
  });

  // Keep the last ~4 KiB of stderr so we can dump it on crash.
  let stderrTail = '';
  rustProcess.stderr?.on('data', (d: Buffer) => {
    const text = d.toString();
    stderrTail = (stderrTail + text).slice(-4096);
    console.error(`[rust] ${text}`);
  });
  rustProcess.on('exit', (code, signal) => {
    const reason = signal ? `signal=${signal}` : `code=${code}`;
    if (code !== 0 && !signal) {
      console.error(`Rust crashed (${reason}). stderr tail:\n${stderrTail}`);
    } else {
      console.log(`Rust exited (${reason})`);
    }
  });
}

function sendRpc(method: string, params?: Record<string, unknown>): Promise<unknown> {
  return new Promise((resolve, reject) => {
    if (!rustProcess?.stdin) { reject(new Error('Rust not ready')); return; }
    const id = ++requestId;
    pendingRequests.set(id, { resolve, reject });
    rustProcess.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params: params || {} }) + '\n');
    setTimeout(() => { if (pendingRequests.has(id)) { pendingRequests.delete(id); reject(new Error('timeout')); } }, 180000);
  });
}

function setupIpc() {
  ipcMain.handle('session:list', () => sendRpc('session.list'));
  ipcMain.handle('session:create', () => sendRpc('session.create'));
  ipcMain.handle('session:get', (_e, p: { sessionId: string }) => sendRpc('session.get', p));
  ipcMain.handle('session:delete', (_e, p: { sessionId: string }) => sendRpc('session.delete', p));

  // chat.send — auto-attach config from Electron's config store.
  ipcMain.handle('chat:send', (_e, p: { message: string; sessionId: string }) => {
    const config = readConfig();
    config.api_key = readEncryptedKey() || '';
    return sendRpc('chat.send', { ...p, config });
  });

  // config:get — reads from Electron-managed config.json + secrets.json.
  ipcMain.handle('config:get', () => {
    const config = readConfig();
    const plain = readEncryptedKey();
    (config as any).has_api_key = !!plain;
    if (plain) {
      config.api_key = plain.length > 8
        ? plain.slice(0, 4) + '**' + plain.slice(-4)
        : '***';
    }
    return config;
  });

  // config:set — writes to Electron config.json + secrets.json.
  ipcMain.handle('config:set', (_e, cfg: Record<string, unknown>) => {
    const plaintext = (cfg.api_key as string)?.trim();
    if (plaintext && !plaintext.includes('*')) {
      writeEncryptedKey(plaintext);
    }
    const { api_key, ...rest } = cfg;
    writeConfig(rest);
    return { ok: true };
  });

  // config:probe — tests an LLM endpoint from the main process (has proxy access).
  ipcMain.handle('config:probe', async (_e, p: { base_url: string; model: string; api_key: string; api_protocol: string }) => {
    // If the frontend sent an empty or masked key, use the real one from secrets.
    let key = p.api_key || '';
    if (!key || key.includes('*')) {
      key = readEncryptedKey() || '';
    }
    if (!key) return { ok: false, error: 'No API key configured' };
    const base = p.base_url.replace(/\/+$/, '');
    const isAnthropic = p.api_protocol === 'anthropic';
    const url = isAnthropic ? `${base}/v1/models?limit=1` : `${base}/models`;
    const headers: Record<string, string> = isAnthropic
      ? { 'x-api-key': key, 'anthropic-version': '2023-06-01' }
      : { Authorization: `Bearer ${key}` };
    try {
      const resp = await fetch(url, { headers, signal: AbortSignal.timeout(10000) });
      if (resp.ok || resp.status === 429) return { ok: true };
      if (resp.status === 401 || resp.status === 403) return { ok: false, error: 'Invalid API key' };
      return { ok: false, error: `HTTP ${resp.status}` };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  });

  // Open external URL in the system default browser.
  ipcMain.handle('shell:open-external', async (_e, url: string) => {
    if (typeof url !== 'string') return { ok: false, error: 'invalid url' };
    if (!/^https?:\/\//i.test(url)) return { ok: false, error: 'only http(s) allowed' };
    await shell.openExternal(url);
    return { ok: true };
  });
}

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1000, height: 700, minWidth: 600, minHeight: 400,
    webPreferences: { preload: path.join(__dirname, '../preload/index.js'), contextIsolation: true, nodeIntegration: false },
    title: 'ClawTao',
  });
  isDev ? mainWindow.loadURL('http://localhost:5173') : mainWindow.loadFile(path.join(__dirname, '../../dist/index.html'));
  isDev && mainWindow.webContents.openDevTools();
  mainWindow.on('closed', () => { mainWindow = null; });
}

async function waitForRustReady(maxRetries = 30): Promise<void> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      await sendRpc('ping');
      console.log('Rust backend ready');
      return;
    } catch {
      await new Promise(r => setTimeout(r, 500));
    }
  }
  console.error('Rust backend failed to start');
}

app.disableHardwareAcceleration();
setupIpc();

app.whenReady().then(async () => {
  console.log('ClawTao starting...');
  startBrowserServer();
  startRust();
  await waitForRustReady();
  await createWindow();
  app.on('activate', async () => { if (BrowserWindow.getAllWindows().length === 0) await createWindow(); });
});

app.on('window-all-closed', () => { rustProcess?.kill(); if (process.platform !== 'darwin') app.quit(); });
app.on('before-quit', () => { rustProcess?.kill(); });
