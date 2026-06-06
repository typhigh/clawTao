import { app, BrowserWindow, ipcMain } from 'electron';
import path from 'path';
import { spawn, ChildProcess } from 'child_process';
import * as readline from 'readline';

let mainWindow: BrowserWindow | null = null;
let rustProcess: ChildProcess | null = null;
const pendingRequests = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
let requestId = 0;

const isDev = process.env.NODE_ENV !== 'production' || !app.isPackaged;

function startRust() {
  const manifestPath = path.join(__dirname, '../../core/Cargo.toml');
  const coreDir = path.dirname(manifestPath);

  rustProcess = spawn('cargo', ['run', '--manifest-path', manifestPath], {
    cwd: coreDir,
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, RUST_LOG: 'debug' },
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
        // Notification from Rust → forward to renderer
        // e.g. Rust "chat.text_delta" → IPC "chat:text_delta"
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
  ipcMain.handle('chat:send', (_e, p: { message: string; sessionId: string }) => sendRpc('chat.send', p));
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

app.disableHardwareAcceleration();
setupIpc();

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

app.whenReady().then(async () => {
  console.log('ClawTao starting...');
  startRust();
  await waitForRustReady();
  await createWindow();
  app.on('activate', async () => { if (BrowserWindow.getAllWindows().length === 0) await createWindow(); });
});

app.on('window-all-closed', () => { rustProcess?.kill(); if (process.platform !== 'darwin') app.quit(); });
app.on('before-quit', () => { rustProcess?.kill(); });
