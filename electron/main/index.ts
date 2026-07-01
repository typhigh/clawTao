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

type SecretsFile = {
  providers: Record<string, string>;
};

function readSecrets(): SecretsFile {
  try {
    return JSON.parse(fs.readFileSync(secretsPath(), 'utf-8')) as SecretsFile;
  } catch {
    return { providers: {} };
  }
}

function writeSecrets(secrets: SecretsFile): void {
  fs.writeFileSync(secretsPath(), JSON.stringify(secrets));
}

function readEncryptedKey(providerId: string): string | null {
  const data = readSecrets();
  if (!safeStorage.isEncryptionAvailable()) return null;
  if (data.providers && data.providers[providerId]) {
    return safeStorage.decryptString(Buffer.from(data.providers[providerId], 'base64'));
  }
  return null;
}

function writeEncryptedKey(plaintext: string, providerId: string): void {
  const data = readSecrets();
  const cipher = safeStorage.isEncryptionAvailable()
    ? safeStorage.encryptString(plaintext).toString('base64')
    : Buffer.from(plaintext).toString('base64');
  if (!safeStorage.isEncryptionAvailable()) {
    console.warn('safeStorage unavailable — storing key as plaintext');
  }
  data.providers = { ...(data.providers || {}), [providerId]: cipher };
  writeSecrets(data);
}

function maskKey(plain: string): string {
  return plain.length > 8 ? plain.slice(0, 4) + '********' + plain.slice(-4) : '***';
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

  // Read log_level from our Electron-managed config.json and forward it to the
  // Rust child via RUST_LOG (EnvFilter picks it up on init).
  const cfg = readConfig() as { log_level?: string };
  const logLevel = cfg.log_level || 'info';

  rustProcess = spawn('cargo', ['run', '--manifest-path', manifestPath], {
    cwd: coreDir,
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, RUST_BACKTRACE: '1', RUST_LOG: logLevel },
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

  // chat.interrupt
  ipcMain.handle('chat:interrupt', (_e, p: { sessionId: string }) =>
    sendRpc('chat.interrupt', p)
  );

  // chat.send — resolve per-session model_key to provider/model/api_key.
  ipcMain.handle('chat:send', (_e, p: { message: string; sessionId: string; model_key?: string }) => {
    const config: any = readConfig();
    const key = p.model_key || config.default_model_id || '';
    if (!key) return Promise.reject(new Error('No model selected.'));
    const [providerId, ...rest] = key.split('/');
    const model = rest.join('/');
    const provider = config.providers?.find((pr: any) => pr.id === providerId);
    if (!provider) return Promise.reject(new Error(`Provider not found: ${providerId}`));

    config.api_key = readEncryptedKey(providerId) || '';
    config.base_url = provider.base_url || '';
    config.model = model;
    config.api_protocol = provider.api_protocol || 'anthropic';
    return sendRpc('chat.send', { ...p, config });
  });

  // config:get — reads from Electron-managed config.json + secrets.json.
  // Schema is the new multi-provider shape only. Per-provider: we set
  // `providers[i].api_key` to a masked string and never expose plaintext
  // to the renderer.
  ipcMain.handle('config:get', () => {
    const config: any = readConfig();
    config.providers = (config.providers || []).map((p: any) => {
      const plain = readEncryptedKey(p.id);
      return {
        ...p,
        api_key: plain ? maskKey(plain) : '',
        has_api_key: !!plain,
      };
    });
    return config;
  });

  // config:set — writes to Electron config.json + secrets.json.
  // cfg.providers[i].api_key: masked (with '*') or undefined means "don't change".
  // Plaintext (no '*') means "set this provider's key to that value".
  // Explicit null means "remove this provider's key".
  ipcMain.handle('config:set', (_e, cfg: Record<string, unknown>) => {
    const providers = cfg.providers as any[] | undefined;
    if (Array.isArray(providers)) {
      providers.forEach(p => {
        if (p.api_key === null) {
          const data = readSecrets();
          if (data.providers) {
            delete data.providers[p.id];
            writeSecrets(data);
          }
          return;
        }
        const k = (p.api_key as string | undefined)?.trim();
        if (k && !k.includes('*')) {
          writeEncryptedKey(k, p.id);
        }
        // masked or empty/missing => leave secrets untouched
      });
    }

    // If log_level changed, push it to the running Rust process so tracing
    // updates without an app restart.
    const newLevel = cfg.log_level as string | undefined;
    if (newLevel) {
      try {
        const prev = readConfig();
        if (prev.log_level !== newLevel) {
          sendRpc('config.set_log_level', { level: newLevel }).catch((e) => {
            console.error('Failed to push log_level to rust:', e);
          });
        }
      } catch { /* readConfig already swallows */ }
    }

    writeConfig(cfg);
    return { ok: true };
  });

  // config:probe — tests an LLM endpoint from the main process (has proxy access).
  // Behavior:
  //   - If the caller provided a non-masked api_key, use it (in-progress edit).
  //   - Otherwise look up the saved key for `provider_id` (the only correct
  //     fallback: it keeps each provider's key isolated).
  ipcMain.handle('config:probe', async (_e, p: { base_url: string; model: string; api_key: string; api_protocol: string; provider_id?: string }) => {
    let key = (p.api_key || '').trim();
    if (!key || key.includes('*')) {
      if (p.provider_id) key = readEncryptedKey(p.provider_id) || '';
    }
    if (!key) return { ok: false, error: 'No API key' };
    const base = p.base_url.replace(/\/+$/, '');
    const isAnthropic = p.api_protocol === 'anthropic';
    // Probe by sending a minimal chat request to the actual LLM endpoint.
    // We deliberately do NOT use a model-listing endpoint (e.g. /v1/models) because
    // some Anthropic-compatible providers (like DeepSeek's anthropic mirror) don't
    // expose that path and would 404 even though the chat endpoint works fine.
    const url = isAnthropic ? `${base}/v1/messages` : `${base}/chat/completions`;
    const headers: Record<string, string> = isAnthropic
      ? { 'x-api-key': key, 'anthropic-version': '2023-06-01', 'content-type': 'application/json' }
      : { Authorization: `Bearer ${key}`, 'content-type': 'application/json' };
    const model = p.model || '';
    const body = JSON.stringify({ model, max_tokens: 1, messages: [{ role: 'user', content: 'hi' }] });
    console.log('[config:probe] request', { url, method: 'POST', headers, base_url: p.base_url, api_protocol: p.api_protocol, provider_id: p.provider_id, model });
    try {
      const resp = await fetch(url, { method: 'POST', headers, body, signal: AbortSignal.timeout(15000) });
      const respHeaders: Record<string, string> = {};
      resp.headers.forEach((v, k) => { respHeaders[k] = v; });
      const respBody = await resp.text();
      console.log('[config:probe] response', { status: resp.status, ok: resp.ok, headers: respHeaders, body: respBody.slice(0, 2000) });
      if (resp.ok) return { ok: true };
      if (resp.status === 401 || resp.status === 403) return { ok: false, error: 'Invalid API key' };
      if (resp.status === 404) return { ok: false, error: `HTTP 404 — check base_url (got ${url})` };
      if (resp.status === 429) return { ok: true }; // rate-limit means auth+endpoint are fine
      return { ok: false, error: `HTTP ${resp.status}` };
    } catch (e) {
      console.log('[config:probe] fetch error', { url, error: String(e) });
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
