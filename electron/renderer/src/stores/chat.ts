/**
 * Chat Store — central state for the chat UI.
 *
 * All streaming events arrive through a single `chat.stream` IPC channel.
 * The unified `StreamEvent` type carries a `kind` discriminator so the
 * frontend receives a time-ordered stream that mirrors the backend's
 * execution order — no stitching or guessing required.
 */
import { create } from 'zustand';
import { useSettingsStore } from './settings';

declare global {
  interface Window {
    electronAPI: {
      chat: {
        send: (message: string, sessionId: string, model_key?: string, thinking_enabled?: boolean, images?: ImageAttachment[]) => Promise<{
          runId: string;
          message: Message;
        }>;
        interrupt: (sessionId: string) => Promise<unknown>;
      };
      session: {
        list: () => Promise<Session[]>;
        create: () => Promise<Session>;
        get: (sessionId: string) => Promise<Session>;
        delete: (sessionId: string) => Promise<unknown>;
      };
      config: {
        get: () => Promise<unknown>;
        set: (c: unknown) => Promise<unknown>;
        probe: (p: { base_url: string; model: string; api_key: string; api_protocol: string }) => Promise<{ ok: boolean; error?: string }>;
      };
      /** Unified stream listener — all turn events arrive through this single channel. */
      onStreamEvent: (callback: (params: StreamEvent) => void) => void;
      image: {
        get: (p: { path: string }) => Promise<{ ok: boolean; base64?: string; mediaType?: string }>;
      };
      /** Open URL in system default browser (not Electron's built-in one). */
      shell: {
        openExternal: (url: string) => Promise<{ ok: boolean; error?: string }>;
      };
    };
  }
}

// ── Data types ────────────────────────────────────────────────────────

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  timestamp: number;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
  thinking?: string;
  /** Images attached to this message (user uploads, ephemeral). */
  images?: ImageAttachment[];
  /** Filesystem paths to persisted images. */
  image_paths?: string[];
}

export interface ToolCall {
  id: string;
  type: string;
  function: {
    name: string;
    arguments: string;
  };
}

export interface Session {
  id: string;
  created_at: number;
  updated_at: number;
  title?: string;
  messages: Message[];
  /** Per-session live stream state — no cross-talk between sessions. */
  currentTurn?: StreamEvent[];
  isStreaming?: boolean;
  currentRunId?: string | null;
  /** Per-session model selection — "providerId/modelName" format. */
  model_key?: string;
  /** Per-session thinking toggle state. */
  thinking_enabled?: boolean;
}

// ── Unified stream event ──────────────────────────────────────────────

/**
 * Every event in a single agent turn is delivered through `chat.stream`
 * with a `kind` field. The stream is strictly ordered by the backend:
 *
 *   started → (text | tool_call → tool_result)* → done
 *
 * The frontend only needs to append events to an array and render them
 * in sequence — no timeline reconstruction, no stitching of runningTools.
 */
export type StreamEvent = {
  sessionId: string;
  runId: string;
  kind: 'started' | 'text' | 'thinking' | 'todo' | 'tool_call' | 'tool_result' | 'done';
  // text / thinking
  delta?: string;
  // tool_call
  toolCallId?: string;
  toolName?: string;
  input?: unknown;
  // tool_result
  output?: string;
  // todo
  todos?: { step: string; status: string }[];
};

// ── Image attachment ──────────────────────────────────────────────────

export interface ImageAttachment {
  base64: string;
  mediaType: string; // "image/png" | "image/jpeg" | "image/gif" | "image/webp"
}

// ── Structured error ──────────────────────────────────────────────────

/**
 * Structured chat error surface.
 *
 * When the Rust backend rejects with a known error code, the main process
 * attaches it to the Error as extra properties so the renderer can render
 * differentiated recovery UI instead of a single generic banner.
 */
export interface ChatError {
  message: string;
  /** Stable snake_case code from the backend, e.g. "UNAUTHORIZED". */
  errorCode: string;
  /** Whether the error is transient and worth offering a retry affordance. */
  retryable: boolean;
}

// ── Store ─────────────────────────────────────────────────────────────

interface ChatState {
  sessions: Session[];
  activeSessionId: string | null;
  error: ChatError | null;

  // Actions
  loadSessions: () => Promise<void>;
  createSession: () => Promise<void>;
  selectSession: (sessionId: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  sendMessage: (text: string, thinkingEnabled?: boolean, images?: ImageAttachment[]) => Promise<void>;
  cancelRun: () => Promise<void>;
  setSessionModel: (sessionId: string, modelKey: string) => void;
  setSessionThinking: (sessionId: string, enabled: boolean) => void;
  /** Single handler for all stream events — dispatches on `kind`. */
  handleStreamEvent: (ev: StreamEvent) => void;
  clearError: () => void;
}

// Helper: update a single session in-place.
const patchSession = (
  sessions: Session[],
  sessionId: string,
  fn: (s: Session) => Session,
) => sessions.map((s) => (s.id === sessionId ? fn(s) : s));

/** Load images from filesystem paths into base64 for display. */
async function loadImagesForSession(session: Session): Promise<Session> {
  for (const msg of session.messages) {
    if (msg.image_paths?.length && !msg.images?.length) {
      const imgs: ImageAttachment[] = [];
      for (const p of msg.image_paths) {
        const r = await window.electronAPI.image.get({ path: p });
        if (r.ok && r.base64) imgs.push({ base64: r.base64, mediaType: r.mediaType || 'image/png' });
      }
      if (imgs.length) msg.images = imgs;
    }
  }
  return session;
}

/** Normalize any caught value into a structured `ChatError`. */
function toChatError(err: unknown): ChatError {
  if (err instanceof Error) {
    const extra = err as Error & { errorCode?: string; retryable?: boolean };
    return {
      message: err.message,
      errorCode: extra.errorCode ?? 'INTERNAL_ERROR',
      retryable: extra.retryable ?? false,
    };
  }
  return { message: String(err), errorCode: 'INTERNAL_ERROR', retryable: false };
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  error: null,

  loadSessions: async () => {
    try {
      const sessions = await window.electronAPI.session.list();
      // Restore session model preferences from localStorage.
      let saved: Record<string, string> = {};
      let savedThinking: Record<string, boolean> = {};
      try { saved = JSON.parse(localStorage.getItem('clawtao-session-models') || '{}'); } catch {}
      try { savedThinking = JSON.parse(localStorage.getItem('clawtao-session-thinking') || '{}'); } catch {}
      for (const s of sessions) {
        if (saved[s.id]) s.model_key = saved[s.id];
        if (typeof savedThinking[s.id] === 'boolean') s.thinking_enabled = savedThinking[s.id];
        await loadImagesForSession(s);
      }
      set({ sessions });
      if (sessions.length === 0) {
        await get().createSession();
      } else if (!get().activeSessionId) {
        await get().selectSession(sessions[0].id);
      }
    } catch (error) {
      set({ error: toChatError(error) });
    }
  },

  createSession: async () => {
    try {
      const session = await window.electronAPI.session.create();
      // Inherit the default model so the session works immediately.
      const defaultModel = (useSettingsStore as any)?.getState?.()?.config?.llm?.default_model_id || '';
      set((state) => ({
        sessions: [{ ...session, model_key: defaultModel || undefined, thinking_enabled: true }, ...state.sessions],
        activeSessionId: session.id,
      }));
    } catch (error) {
      set({ error: toChatError(error) });
    }
  },

  selectSession: async (sessionId: string) => {
    try {
      let session = await window.electronAPI.session.get(sessionId);
      session = await loadImagesForSession(session);
      set((state) => ({
        sessions: state.sessions.map((s) => {
          if (s.id !== sessionId) return s;
          return { ...session, currentTurn: s.currentTurn, isStreaming: s.isStreaming, currentRunId: s.currentRunId, model_key: s.model_key, thinking_enabled: s.thinking_enabled };
        }),
        activeSessionId: sessionId,
      }));
    } catch (error) {
      set({ error: toChatError(error) });
    }
  },

  deleteSession: async (sessionId: string) => {
    try {
      await window.electronAPI.session.delete(sessionId);
      // Clean up persisted model preference.
      try {
        const saved = JSON.parse(localStorage.getItem('clawtao-session-models') || '{}');
        delete saved[sessionId];
        localStorage.setItem('clawtao-session-models', JSON.stringify(saved));
      } catch {}
      const state = get();
      const sessions = state.sessions.filter(s => s.id !== sessionId);
      const activeSessionId = state.activeSessionId === sessionId
        ? (sessions[0]?.id || null)
        : state.activeSessionId;
      set({ sessions, activeSessionId });
      // Clean up persisted preferences.
      try {
        const saved = JSON.parse(localStorage.getItem('clawtao-session-models') || '{}');
        delete saved[sessionId];
        localStorage.setItem('clawtao-session-models', JSON.stringify(saved));
        const savedT = JSON.parse(localStorage.getItem('clawtao-session-thinking') || '{}');
        delete savedT[sessionId];
        localStorage.setItem('clawtao-session-thinking', JSON.stringify(savedT));
      } catch {}
    } catch (error) {
      set({ error: toChatError(error) });
    }
  },

  sendMessage: async (text: string, thinkingEnabled?: boolean, images?: ImageAttachment[]) => {
    const { activeSessionId } = get();
    if (!activeSessionId) {
      set({ error: { message: 'No active session', errorCode: 'SESSION_ERROR', retryable: false } });
      return;
    }

    const userMsg: Message = {
      id: `tmp-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now(),
      images: images && images.length > 0 ? images : undefined,
    };
    set((state) => ({
      error: null,
      sessions: patchSession(state.sessions, activeSessionId, (s) => ({
        ...s,
        isStreaming: true,
        currentTurn: [],
        messages: [...s.messages, userMsg],
      })),
    }));

    try {
      const session = get().sessions.find(s => s.id === activeSessionId);
      await window.electronAPI.chat.send(text, activeSessionId, session?.model_key, thinkingEnabled, images);
      // handleStreamEvent 'done' already reloaded via session.get; no second reload needed.
    } catch (error) {
      set((state) => ({
        error: toChatError(error),
        sessions: patchSession(state.sessions, activeSessionId, (s) => ({
          ...s,
          isStreaming: false,
          currentTurn: [],
          messages: s.messages.filter(m => !m.id.startsWith('tmp-')),
        })),
      }));
    }
  },

  handleStreamEvent: (ev: StreamEvent) => {
    set((state) => {
      const sid = ev.sessionId;
      switch (ev.kind) {
        case 'started':
          return {
            sessions: patchSession(state.sessions, sid, (s) => ({
              ...s,
              isStreaming: true,
              currentTurn: [ev],
              currentRunId: ev.runId,
            })),
          };
        case 'text':
        case 'thinking':
        case 'todo':
        case 'tool_call':
        case 'tool_result':
          return {
            sessions: patchSession(state.sessions, sid, (s) => ({
              ...s,
              currentTurn: [...(s.currentTurn || []), ev],
            })),
          };
        case 'done':
          // Preserve user-uploaded images from the temp user message.
          const tempUserImages = (state.sessions
            .find(s => s.id === sid)?.messages || [])
            .filter(m => m.id.startsWith('tmp-') && m.role === 'user')
            .flatMap(m => m.images || []);
          // Async-reload from Rust, then merge preserved images.
          window.electronAPI.session.get(sid).then((session) => {
            if (tempUserImages.length > 0) {
              const msgs = [...session.messages];
              for (let i = msgs.length - 1; i >= 0; i--) {
                if (msgs[i].role === 'user') { msgs[i] = { ...msgs[i], images: tempUserImages }; break; }
              }
              session.messages = msgs;
            }
            set((s2) => ({
              sessions: patchSession(s2.sessions, sid, () => ({
                ...session,
                isStreaming: false,
                currentTurn: [],
                model_key: s2.sessions.find(x => x.id === sid)?.model_key,
              })),
            }));
          }).catch(() => {
            set((s2) => ({
              sessions: patchSession(s2.sessions, sid, (s) => ({
                ...s,
                isStreaming: false,
                currentTurn: [],
              })),
            }));
          });
          return {
            sessions: patchSession(state.sessions, sid, (s) => ({
              ...s,
              currentTurn: [...(s.currentTurn || []), ev],
            })),
          };
        default:
          return {};
      }
    });
  },

  cancelRun: async () => {
    const sid = get().activeSessionId;
    if (!sid) return;
    try { await window.electronAPI.chat.interrupt(sid); } catch { /* ignore */ }
  },

  setSessionModel: (sessionId, modelKey) => {
    set((state) => ({
      sessions: state.sessions.map((s) => s.id === sessionId ? { ...s, model_key: modelKey } : s),
    }));
    // Persist across restarts.
    try {
      const saved = JSON.parse(localStorage.getItem('clawtao-session-models') || '{}');
      saved[sessionId] = modelKey;
      localStorage.setItem('clawtao-session-models', JSON.stringify(saved));
    } catch {}
  },

  setSessionThinking: (sessionId, enabled) => {
    set((state) => ({
      sessions: state.sessions.map((s) => s.id === sessionId ? { ...s, thinking_enabled: enabled } : s),
    }));
    // Persist across restarts.
    try {
      const saved = JSON.parse(localStorage.getItem('clawtao-session-thinking') || '{}');
      saved[sessionId] = enabled;
      localStorage.setItem('clawtao-session-thinking', JSON.stringify(saved));
    } catch {}
  },

  clearError: () => set({ error: null }),
}));
