/**
 * Preload Script — security boundary between main and renderer.
 *
 * Only the methods listed here are available to the renderer via
 * `window.electronAPI`. Every IPC call is routed through a named
 * channel; no arbitrary `ipcRenderer.invoke()` or `ipcRenderer.on()`
 * is exposed directly.
 */
import { contextBridge, ipcRenderer } from 'electron';

const electronAPI = {
  // Chat operations
  chat: {
    send: (message: string, sessionId: string) =>
      ipcRenderer.invoke('chat:send', { message, sessionId }),
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

  // Config operations
  config: {
    get: () => ipcRenderer.invoke('config:get'),
    set: (c: unknown) => ipcRenderer.invoke('config:set', c),
    validate: () => ipcRenderer.invoke('config:validate'),
  },

  // Event listeners for tool calls
  onToolStarted: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:tool_started', (_event, params) => callback(params)),
  onToolResult: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:tool_result', (_event, params) => callback(params)),
};

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

export type ElectronAPI = typeof electronAPI;
