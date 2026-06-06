/**
 * Preload Script
 *
 * Exposes safe APIs to the renderer process via contextBridge
 */

import { contextBridge, ipcRenderer } from 'electron';

// Valid IPC channels (whitelist)
const validInvokeChannels = [
  'chat:send',
  'chat:history',
  'session:list',
  'session:create',
  'session:get',
];

const validReceiveChannels = [
  'chat:started',
  'chat:text_delta',
  'chat:done',
  'rust:exited',
];

// Expose safe API to renderer
const electronAPI = {
  // IPC invoke (request/response)
  invoke: (channel: string, ...args: unknown[]) => {
    if (!validInvokeChannels.includes(channel)) {
      throw new Error(`Invalid IPC channel: ${channel}`);
    }
    return ipcRenderer.invoke(channel, ...args);
  },

  // IPC receive (one-way events from main)
  on: (channel: string, callback: (...args: unknown[]) => void) => {
    if (!validReceiveChannels.includes(channel)) {
      throw new Error(`Invalid IPC channel: ${channel}`);
    }
    const subscription = (_event: Electron.IpcRendererEvent, ...args: unknown[]) => callback(...args);
    ipcRenderer.on(channel, subscription);
    return () => {
      ipcRenderer.removeListener(channel, subscription);
    };
  },

  // Remove listener
  off: (channel: string, callback: (...args: unknown[]) => void) => {
    if (!validReceiveChannels.includes(channel)) {
      throw new Error(`Invalid IPC channel: ${channel}`);
    }
    ipcRenderer.removeListener(channel, callback);
  },

  // Chat operations
  chat: {
    send: (message: string, sessionId: string) =>
      ipcRenderer.invoke('chat:send', { message, sessionId }),
    history: (sessionId: string) =>
      ipcRenderer.invoke('chat:history', { sessionId }),
  },

  // Session operations
  session: {
    list: () => ipcRenderer.invoke('session:list'),
    create: () => ipcRenderer.invoke('session:create'),
    get: (sessionId: string) => ipcRenderer.invoke('session:get', { sessionId }),
  },

  // Event listeners for streaming
  onChatStarted: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:started', (_event, params) => callback(params)),
  onTextDelta: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:text_delta', (_event, params) => callback(params)),
  onChatDone: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:done', (_event, params) => callback(params)),
  onRustExited: (callback: (params: unknown) => void) =>
    ipcRenderer.on('rust:exited', (_event, params) => callback(params)),
};

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

// TypeScript type declaration for renderer
export type ElectronAPI = typeof electronAPI;
