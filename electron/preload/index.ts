/**
 * Preload Script
 *
 * Exposes safe APIs to the renderer process via contextBridge
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

  // Event listeners for tool calls
  onToolStarted: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:tool_started', (_event, params) => callback(params)),
  onToolResult: (callback: (params: unknown) => void) =>
    ipcRenderer.on('chat:tool_result', (_event, params) => callback(params)),
};

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

export type ElectronAPI = typeof electronAPI;
