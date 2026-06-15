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

async function injectKeyIntoRust(key: string): Promise<void> {
  await sendRpc('config.injectKey', { api_key: key });
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
    env: { ...process.env },
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

  rustProcess.stderr?.on('data', (d: Buffer) => console.error(`[rust] ${d}`));
  rustProcess.on('exit', (code) => console.log(`Rust exited: ${code}`));
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
  ipcMain.handle('chat:send', (_e, p: { message: string; sessionId: string }) => sendRpc('chat.send', p));

  // config:get — returns masked config, adds has_api_key flag
  ipcMain.handle('config:get', async () => {
    const config = await sendRpc('config.get') as Record<string, unknown>;
    (config as any).has_api_key = !!readEncryptedKey();
    return config;
  });

  // config:set — always sends complete config with real api_key to Rust
  ipcMain.handle('config:set', async (_e, cfg: Record<string, unknown>) => {
    const plaintext = (cfg.api_key as string)?.trim();
    if (plaintext && !plaintext.includes('*')) {
      // User typed a new key — encrypt and use it
      writeEncryptedKey(plaintext);
      cfg.api_key = plaintext;
    } else {
      // No new key provided — re-use the existing one
      cfg.api_key = readEncryptedKey() || '';
    }
    return sendRpc('config.set', cfg);
  });

  ipcMain.handle('config:validate', () => sendRpc('config.validate'));
  ipcMain.handle('config:testKey', (_e, p: { api_key: string; base_url: string; model: string }) => sendRpc('config.testKey', p));

  // Open external URL in the system default browser (not Electron's built-in one).
  // See https://www.electronjs.org/docs/latest/api/shell#shellopenexternalurl-options
  ipcMain.handle('shell:open-external', async (_e, url: string) => {
    if (typeof url !== 'string') return { ok: false, error: 'invalid url' };
    // Only allow http(s) — never let a tool-attacker call file:// or shell-protocol.
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

  // Inject API key from encrypted secrets.json into Rust
  const key = readEncryptedKey();
  if (key) {
    try { await injectKeyIntoRust(key); } catch (e) { console.error('Failed to inject key:', e); }
  }

  await createWindow();
  app.on('activate', async () => { if (BrowserWindow.getAllWindows().length === 0) await createWindow(); });
});

app.on('window-all-closed', () => { rustProcess?.kill(); if (process.platform !== 'darwin') app.quit(); });
app.on('before-quit', () => { rustProcess?.kill(); });
