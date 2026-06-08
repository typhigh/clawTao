/**
 * Chat Store — central state for the chat UI.
 *
 * Manages sessions, messages, streaming text, and currently-running tool calls.
 * IPC events from main (chat.text_delta, chat.tool_started, etc.) update this
 * store reactively so the UI re-renders while the LLM is still generating.
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
        testKey: (p: { api_key: string; base_url: string; model: string }) => Promise<{ ok: boolean; error?: string }>;
      };
      onChatStarted: (callback: (params: unknown) => void) => void;
      onTextDelta: (callback: (params: { sessionId: string; runId: string; delta: string }) => void) => void;
      onChatDone: (callback: (params: unknown) => void) => void;
      onToolStarted: (callback: (params: ToolCallEvent) => void) => void;
      onToolResult: (callback: (params: ToolResultEvent) => void) => void;
    };
  }
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  timestamp: number;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
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
}

export interface ToolCallEvent {
  sessionId: string;
  runId: string;
  toolCallId: string;
  toolName: string;
  toolInput: unknown;
}

export interface ToolResultEvent {
  sessionId: string;
  runId: string;
  toolCallId: string;
  toolName: string;
  result: string;
}

interface RunningTool {
  toolCallId: string;
  toolName: string;
  toolInput: unknown;
  result: string | null;
}

interface ChatState {
  sessions: Session[];
  activeSessionId: string | null;
  streamingText: string;
  isStreaming: boolean;
  currentRunId: string | null;
  runningTools: RunningTool[];
  error: string | null;

  // Actions
  loadSessions: () => Promise<void>;
  createSession: () => Promise<void>;
  selectSession: (sessionId: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  sendMessage: (text: string) => Promise<void>;
  handleTextDelta: (params: { sessionId: string; delta: string }) => void;
  handleChatDone: () => void;
  handleChatStarted: (params: { sessionId: string; runId: string }) => void;
  handleToolStarted: (params: ToolCallEvent) => void;
  handleToolResult: (params: ToolResultEvent) => void;
  clearError: () => void;
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  streamingText: '',
  isStreaming: false,
  currentRunId: null,
  runningTools: [],
  error: null,

  loadSessions: async () => {
    try {
      const sessions = await window.electronAPI.session.list();
      set({ sessions });
      if (sessions.length > 0 && !get().activeSessionId) {
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
        sessions: state.sessions.map((s) => (s.id === sessionId ? session : s)),
        activeSessionId: sessionId,
        streamingText: '',
        isStreaming: false,
        runningTools: [],
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
      set({ sessions, activeSessionId, streamingText: '', isStreaming: false, runningTools: [] });
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

    set({ isStreaming: true, error: null, runningTools: [] });

    try {
      await window.electronAPI.chat.send(text, activeSessionId);
      const session = await window.electronAPI.session.get(activeSessionId);
      set((state) => ({
        sessions: state.sessions.map((s) => (s.id === activeSessionId ? session : s)),
      }));
    } catch (error) {
      set({ error: `Failed to send message: ${error}`, isStreaming: false });
    }
  },

  handleTextDelta: ({ sessionId, delta }) => {
    const { activeSessionId } = get();
    if (sessionId !== activeSessionId) return;
    set((state) => ({
      streamingText: state.streamingText + delta,
    }));
  },

  handleChatDone: () => {
    set((state) => {
      const { activeSessionId, streamingText } = state;
      if (!activeSessionId) return { isStreaming: false, streamingText: '', runningTools: [] };

      // Only add streaming text if there is any (tool-only responses may have none)
      if (streamingText) {
        const newMessage: Message = {
          id: `temp-${Date.now()}`,
          role: 'assistant',
          content: streamingText,
          timestamp: Date.now(),
        };
        return {
          sessions: state.sessions.map((s) =>
            s.id === activeSessionId ? { ...s, messages: [...s.messages, newMessage] } : s
          ),
          streamingText: '',
          isStreaming: false,
          runningTools: [],
        };
      }

      return { streamingText: '', isStreaming: false, runningTools: [] };
    });
  },

  handleChatStarted: ({ sessionId, runId }) => {
    const { activeSessionId } = get();
    if (sessionId !== activeSessionId) return;
    set({ currentRunId: runId, streamingText: '', runningTools: [] });
  },

  handleToolStarted: ({ toolCallId, toolName, toolInput }) => {
    set((state) => ({
      runningTools: [
        ...state.runningTools,
        { toolCallId, toolName, toolInput, result: null },
      ],
    }));
  },

  handleToolResult: ({ toolCallId, result }) => {
    set((state) => ({
      runningTools: state.runningTools.map((t) =>
        t.toolCallId === toolCallId ? { ...t, result } : t
      ),
    }));
  },

  clearError: () => set({ error: null }),
}));
