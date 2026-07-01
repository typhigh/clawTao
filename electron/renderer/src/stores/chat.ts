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
        send: (message: string, sessionId: string, model_key?: string) => Promise<{
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
        get: () => Promise<{ provider: string; api_key: string; base_url: string; model: string; log_level: string; bash_blocked_commands: string[] }>;
        set: (c: unknown) => Promise<unknown>;
        probe: (p: { base_url: string; model: string; api_key: string; api_protocol: string }) => Promise<{ ok: boolean; error?: string }>;
      };
      /** Unified stream listener — all turn events arrive through this single channel. */
      onStreamEvent: (callback: (params: StreamEvent) => void) => void;
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
  kind: 'started' | 'text' | 'thinking' | 'tool_call' | 'tool_result' | 'done';
  // text / thinking
  delta?: string;
  // tool_call
  toolCallId?: string;
  toolName?: string;
  input?: unknown;
  // tool_result
  output?: string;
};

// ── Store ─────────────────────────────────────────────────────────────

interface ChatState {
  sessions: Session[];
  activeSessionId: string | null;
  error: string | null;

  // Actions
  loadSessions: () => Promise<void>;
  createSession: () => Promise<void>;
  selectSession: (sessionId: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  sendMessage: (text: string) => Promise<void>;
  cancelRun: () => Promise<void>;
  setSessionModel: (sessionId: string, modelKey: string) => void;
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

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  error: null,

  loadSessions: async () => {
    try {
      const sessions = await window.electronAPI.session.list();
      // Restore session model preferences from localStorage.
      let saved: Record<string, string> = {};
      try { saved = JSON.parse(localStorage.getItem('clawtao-session-models') || '{}'); } catch {}
      for (const s of sessions) {
        if (saved[s.id]) s.model_key = saved[s.id];
      }
      set({ sessions });
      if (sessions.length === 0) {
        await get().createSession();
      } else if (!get().activeSessionId) {
        await get().selectSession(sessions[0].id);
      }
    } catch (error) {
      set({ error: `Failed to load sessions: ${error}` });
    }
  },

  createSession: async () => {
    try {
      const session = await window.electronAPI.session.create();
      // Inherit the default model so the session works immediately.
      const defaultModel = (useSettingsStore as any)?.getState?.()?.config?.default_model_id || '';
      set((state) => ({
        sessions: [{ ...session, model_key: defaultModel || undefined }, ...state.sessions],
        activeSessionId: session.id,
      }));
    } catch (error) {
      set({ error: `Failed to create session: ${error}` });
    }
  },

  selectSession: async (sessionId: string) => {
    try {
      const session = await window.electronAPI.session.get(sessionId);
      set((state) => ({
        sessions: state.sessions.map((s) => {
          if (s.id !== sessionId) return s;
          // Preserve live streaming state — session.get doesn't carry it.
          return { ...session, currentTurn: s.currentTurn, isStreaming: s.isStreaming, currentRunId: s.currentRunId, model_key: s.model_key };
        }),
        activeSessionId: sessionId,
      }));
    } catch (error) {
      set({ error: `Failed to load session: ${error}` });
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
    } catch (error) {
      set({ error: `Failed to delete session: ${error}` });
    }
  },

  sendMessage: async (text: string) => {
    const { activeSessionId } = get();
    if (!activeSessionId) {
      set({ error: 'No active session' });
      return;
    }

    const userMsg: Message = {
      id: `tmp-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now(),
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
      await window.electronAPI.chat.send(text, activeSessionId, session?.model_key);
      // handleStreamEvent 'done' already reloaded via session.get; no second reload needed.
    } catch (error) {
      set((state) => ({
        error: `Failed to send message: ${error}`,
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
        case 'tool_call':
        case 'tool_result':
          return {
            sessions: patchSession(state.sessions, sid, (s) => ({
              ...s,
              currentTurn: [...(s.currentTurn || []), ev],
            })),
          };
        case 'done':
          // Append 'done' event, then async-reload from Rust.
          window.electronAPI.session.get(sid).then((session) => {
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

  clearError: () => set({ error: null }),
}));
