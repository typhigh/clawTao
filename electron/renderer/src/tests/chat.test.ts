import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock electronAPI
const mockSessionApi = {
  list: vi.fn(),
  create: vi.fn(),
  get: vi.fn(),
  delete: vi.fn(),
};

vi.stubGlobal('window', {
  electronAPI: {
    chat: { send: vi.fn() },
    session: mockSessionApi,
    config: { get: vi.fn(), set: vi.fn(), validate: vi.fn(), testKey: vi.fn() },
    onStreamEvent: vi.fn(),
  },
});

import { useChatStore, StreamEvent, Session } from '../stores/chat';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 's1',
    created_at: 1000,
    updated_at: 2000,
    messages: [{ id: 'm1', role: 'user' as const, content: 'hello', timestamp: 1000 }],
    ...overrides,
  };
}

function makeEvent(overrides: Partial<StreamEvent> = {}): StreamEvent {
  return {
    sessionId: 's1',
    runId: 'r1',
    kind: 'started',
    ...overrides,
  };
}

function currentTurn(state: ReturnType<typeof useChatStore.getState>, sid: string) {
  return state.sessions.find(s => s.id === sid)?.currentTurn || [];
}

describe('chat store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useChatStore.setState({
      sessions: [],
      activeSessionId: null,
      error: null,
    });
  });

  it('starts with empty state', () => {
    const state = useChatStore.getState();
    expect(state.sessions).toEqual([]);
    expect(state.activeSessionId).toBeNull();
  });

  it('loads sessions and selects first', async () => {
    const s1 = makeSession();
    const s2 = makeSession({ id: 's2', messages: [] });
    mockSessionApi.list.mockResolvedValueOnce([s1, s2]);
    mockSessionApi.get.mockResolvedValueOnce(s1);

    await useChatStore.getState().loadSessions();

    const state = useChatStore.getState();
    expect(state.sessions.length).toBe(2);
    expect(state.activeSessionId).toBe('s1');
  });

  it('creates session and sets as active', async () => {
    const s2 = makeSession({ id: 's2', messages: [] });
    mockSessionApi.create.mockResolvedValueOnce(s2);

    await useChatStore.getState().createSession();

    const state = useChatStore.getState();
    expect(state.activeSessionId).toBe('s2');
    expect(state.sessions[0].id).toBe('s2');
  });

  it('selects session by id', async () => {
    mockSessionApi.get.mockResolvedValueOnce(makeSession());

    await useChatStore.getState().selectSession('s1');

    const state = useChatStore.getState();
    expect(state.activeSessionId).toBe('s1');
  });

  it('handleStreamEvent "started" sets per-session streaming', () => {
    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession()],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'started', runId: 'r99' }));

    const state = useChatStore.getState();
    const s1 = state.sessions.find(s => s.id === 's1')!;
    expect(s1.isStreaming).toBe(true);
    expect(s1.currentRunId).toBe('r99');
    expect(s1.currentTurn).toHaveLength(1);
    expect(s1.currentTurn![0].kind).toBe('started');
  });

  it('handleStreamEvent "text" appends to per-session currentTurn', () => {
    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession({ currentTurn: [makeEvent({ kind: 'started' })], isStreaming: true })],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'text', delta: 'Hello' }));
    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'text', delta: ' World' }));

    const turn = currentTurn(useChatStore.getState(), 's1');
    expect(turn).toHaveLength(3);
    expect(turn[1].delta).toBe('Hello');
    expect(turn[2].delta).toBe(' World');
  });

  it('handleStreamEvent receives events for non-active session', () => {
    // Session s2 is not active but should still receive events.
    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [
        makeSession(),
        makeSession({ id: 's2', messages: [], currentTurn: [], isStreaming: true }),
      ],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({ sessionId: 's2', kind: 'text', delta: 's2-text' }));

    const s2Turn = currentTurn(useChatStore.getState(), 's2');
    expect(s2Turn).toHaveLength(1);
    expect(s2Turn[0].delta).toBe('s2-text');
  });

  it('handleStreamEvent "tool_call" and "tool_result" ordered per-session', () => {
    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession({ currentTurn: [makeEvent({ kind: 'started' })], isStreaming: true })],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({
      kind: 'tool_call', toolCallId: 'tc1', toolName: 'Bash', input: { command: 'ls' },
    }));
    useChatStore.getState().handleStreamEvent(makeEvent({
      kind: 'tool_result', toolCallId: 'tc1', toolName: 'Bash', output: 'stdout:\nfile.txt',
    }));

    const turn = currentTurn(useChatStore.getState(), 's1');
    expect(turn).toHaveLength(3);
    expect(turn[1].kind).toBe('tool_call');
    expect(turn[1].toolName).toBe('Bash');
    expect(turn[2].kind).toBe('tool_result');
    expect(turn[2].output).toBe('stdout:\nfile.txt');
  });

  it('handleStreamEvent "done" reloads session and clears streaming', async () => {
    const doneSession = {
      ...makeSession(),
      messages: [
        ...makeSession().messages,
        { id: 'm2', role: 'assistant' as const, content: 'Hi!', timestamp: 3000 },
      ],
    };
    mockSessionApi.get.mockResolvedValueOnce(doneSession);

    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession({
        isStreaming: true,
        currentTurn: [makeEvent({ kind: 'started' }), makeEvent({ kind: 'text', delta: 'Hi!' })],
      })],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'done' }));

    await vi.waitFor(() => {
      const state = useChatStore.getState();
      const s1 = state.sessions.find(s => s.id === 's1')!;
      expect(s1.isStreaming).toBe(false);
      expect(s1.currentTurn).toHaveLength(0);
      expect(s1.messages).toHaveLength(2);
    });
  });

  it('selectSession preserves live streaming state', async () => {
    const freshFromRust = makeSession({ messages: [{ id: 'm99', role: 'assistant' as const, content: 'new', timestamp: 999 }] });
    mockSessionApi.get.mockResolvedValueOnce(freshFromRust);

    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession({ isStreaming: true, currentTurn: [makeEvent({ kind: 'text', delta: 'live' })], currentRunId: 'r99' })],
    });

    await useChatStore.getState().selectSession('s1');

    const s1 = useChatStore.getState().sessions.find(s => s.id === 's1')!;
    expect(s1.isStreaming).toBe(true);
    expect(s1.currentTurn).toHaveLength(1);
    expect(s1.currentRunId).toBe('r99');
    // Messages are refreshed from Rust, but steaming state is preserved.
    expect(s1.messages).toHaveLength(1);
    expect(s1.messages[0].id).toBe('m99');
  });

  it('sendMessage on error removes optimistic user message', async () => {
    vi.mocked(window.electronAPI.chat.send).mockRejectedValueOnce(new Error('network'));
    // Prevent the store from auto-creating a session; test a single-session scenario.
    useChatStore.setState({
      activeSessionId: 's1',
      sessions: [makeSession({ messages: [] })],
    });

    await useChatStore.getState().sendMessage('will fail');

    const state = useChatStore.getState();
    const s1 = state.sessions.find(s => s.id === 's1')!;
    expect(s1.isStreaming).toBe(false);
    expect(s1.messages.filter(m => m.role === 'user')).toHaveLength(0);
    expect(state.error).toContain('Failed to send message');
  });
});
