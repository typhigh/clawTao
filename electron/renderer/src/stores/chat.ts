/**
 * Chat Store — central state for the chat UI.
 *
 * All streaming events arrive through a single `chat.stream` IPC channel.
 * The unified `StreamEvent` type carries a `kind` discriminator so the
 * frontend receives a time-ordered stream that mirrors the backend's
 * execution order — no stitching or guessing required.
 */
import { create } from 'zustand';

declare global {
  interface Window {
    electronAPI: {
      chat: {
        send: (message: string, sessionId: string) => Promise<{
          runId: string;
          message: Message;
        }>;
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
        validate: () => Promise<{ ok: boolean; error?: string }>;
        testKey: (p: { api_key: string; base_url: string; model: string; api_protocol: string }) => Promise<{ ok: boolean; error?: string }>;
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
  messages: Message[];
  /** Per-session live stream state — no cross-talk between sessions. */
  currentTurn?: StreamEvent[];
  isStreaming?: boolean;
  currentRunId?: string | null;
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
      set((state) => ({
        sessions: [session, ...state.sessions],
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
          return { ...session, currentTurn: s.currentTurn, isStreaming: s.isStreaming, currentRunId: s.currentRunId };
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
      await window.electronAPI.chat.send(text, activeSessionId);
      const session = await window.electronAPI.session.get(activeSessionId);
      set((state) => ({
        sessions: patchSession(state.sessions, activeSessionId, () => ({
          ...session,
          isStreaming: false,
          currentTurn: [],
        })),
      }));
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

  clearError: () => set({ error: null }),
}));
