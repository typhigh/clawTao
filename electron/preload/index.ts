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
    send: (message: string, sessionId: string, model_key?: string, thinking_enabled?: boolean, images?: { base64: string; mediaType: string }[]) =>
      ipcRenderer.invoke('chat:send', { message, sessionId, model_key, thinking_enabled, images }),
    interrupt: (sessionId: string) =>
      ipcRenderer.invoke('chat:interrupt', { sessionId }),
  },

  // Session operations
  session: {
    list: () => ipcRenderer.invoke('session:list'),
    create: () => ipcRenderer.invoke('session:create'),
    get: (sessionId: string) => ipcRenderer.invoke('session:get', { sessionId }),
    delete: (sessionId: string) => ipcRenderer.invoke('session:delete', { sessionId }),
  },

  // Unified stream event listener (replaces chat:started / text_delta / tool_started / tool_result / done)
  onStreamEvent: (callback: (params: unknown) => void) => {
    ipcRenderer.removeAllListeners('chat:stream');
    ipcRenderer.on('chat:stream', (_event, params) => callback(params));
  },

  // Config operations (managed by Electron main, no Rust RPC needed)
  config: {
    get: () => ipcRenderer.invoke('config:get'),
    set: (c: unknown) => ipcRenderer.invoke('config:set', c),
    probe: (p: { base_url: string; model: string; api_key: string; api_protocol: string; provider_id?: string | null }) => ipcRenderer.invoke('config:probe', p),
  },

  // Image helpers (read from disk)
  image: {
    get: (p: { path: string }) => ipcRenderer.invoke('image:get', p) as Promise<{ ok: boolean; base64?: string; mediaType?: string }>,
  },

  // Shell helpers (open URL in system default browser)
  shell: {
    openExternal: (url: string) => ipcRenderer.invoke('shell:open-external', url),
  },

};

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

export type ElectronAPI = typeof electronAPI;
