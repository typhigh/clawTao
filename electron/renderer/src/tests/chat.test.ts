import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock electronAPI
const mockSession = {
  list: vi.fn(),
  create: vi.fn(),
  get: vi.fn(),
  delete: vi.fn(),
};

vi.stubGlobal('window', {
  electronAPI: {
    chat: { send: vi.fn() },
    session: mockSession,
    config: { get: vi.fn(), set: vi.fn(), validate: vi.fn(), testKey: vi.fn() },
    onStreamEvent: vi.fn(),
  },
});

import { useChatStore, StreamEvent } from '../stores/chat';

const mockSession1 = {
  id: 's1',
  created_at: 1000,
  updated_at: 2000,
  messages: [
    { id: 'm1', role: 'user' as const, content: 'hello', timestamp: 1000 },
  ],
};

const mockSession2 = {
  id: 's2',
  created_at: 3000,
  updated_at: 4000,
  messages: [],
};

function makeEvent(overrides: Partial<StreamEvent> = {}): StreamEvent {
  return {
    sessionId: 's1',
    runId: 'r1',
    kind: 'started',
    ...overrides,
  };
}

describe('chat store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useChatStore.setState({
      sessions: [],
      activeSessionId: null,
      currentTurn: [],
      isStreaming: false,
      currentRunId: null,
      error: null,
    });
  });

  it('starts with empty state', () => {
    const state = useChatStore.getState();
    expect(state.sessions).toEqual([]);
    expect(state.activeSessionId).toBeNull();
    expect(state.isStreaming).toBe(false);
    expect(state.currentTurn).toEqual([]);
  });

  it('loads sessions and selects first', async () => {
    mockSession.list.mockResolvedValueOnce([mockSession1, mockSession2]);
    mockSession.get.mockResolvedValueOnce(mockSession1);

    await useChatStore.getState().loadSessions();

    const state = useChatStore.getState();
    expect(state.sessions.length).toBe(2);
    expect(state.activeSessionId).toBe('s1');
  });

  it('creates session and sets as active', async () => {
    mockSession.create.mockResolvedValueOnce(mockSession2);

    await useChatStore.getState().createSession();

    const state = useChatStore.getState();
    expect(state.activeSessionId).toBe('s2');
    expect(state.sessions[0].id).toBe('s2');
  });

  it('selects session by id', async () => {
    mockSession.get.mockResolvedValueOnce(mockSession1);

    await useChatStore.getState().selectSession('s1');

    const state = useChatStore.getState();
    expect(state.activeSessionId).toBe('s1');
  });

  it('handleStreamEvent "started" sets streaming and clears currentTurn', () => {
    useChatStore.setState({ activeSessionId: 's1', currentTurn: [{ kind: 'text', sessionId: 's1', runId: 'old', delta: 'old' }] });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'started', runId: 'r99' }));

    const state = useChatStore.getState();
    expect(state.isStreaming).toBe(true);
    expect(state.currentRunId).toBe('r99');
    expect(state.currentTurn).toHaveLength(1);
    expect(state.currentTurn[0].kind).toBe('started');
  });

  it('handleStreamEvent "text" appends to currentTurn', () => {
    useChatStore.setState({ activeSessionId: 's1', isStreaming: true, currentTurn: [makeEvent({ kind: 'started' })] });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'text', delta: 'Hello' }));
    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'text', delta: ' World' }));

    const state = useChatStore.getState();
    expect(state.currentTurn).toHaveLength(3);
    expect(state.currentTurn[1].delta).toBe('Hello');
    expect(state.currentTurn[2].delta).toBe(' World');
  });

  it('handleStreamEvent ignores non-active session', () => {
    useChatStore.setState({ activeSessionId: 's2', currentTurn: [] });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'text', delta: 'nope' }));

    expect(useChatStore.getState().currentTurn).toHaveLength(0);
  });

  it('handleStreamEvent "tool_call" and "tool_result" are ordered in currentTurn', () => {
    useChatStore.setState({ activeSessionId: 's1', isStreaming: true, currentTurn: [makeEvent({ kind: 'started' })] });

    useChatStore.getState().handleStreamEvent(makeEvent({
      kind: 'tool_call', toolCallId: 'tc1', toolName: 'Bash', input: { command: 'ls' },
    }));
    useChatStore.getState().handleStreamEvent(makeEvent({
      kind: 'tool_result', toolCallId: 'tc1', toolName: 'Bash', output: 'stdout:\nfile.txt',
    }));

    const turn = useChatStore.getState().currentTurn;
    expect(turn).toHaveLength(3);
    expect(turn[1].kind).toBe('tool_call');
    expect(turn[1].toolName).toBe('Bash');
    expect(turn[2].kind).toBe('tool_result');
    expect(turn[2].output).toBe('stdout:\nfile.txt');
  });

  it('handleStreamEvent "done" reloads session and clears streaming', async () => {
    const doneSession = {
      ...mockSession1,
      messages: [
        ...mockSession1.messages,
        { id: 'm2', role: 'assistant' as const, content: 'Hi!', timestamp: 3000 },
      ],
    };
    mockSession.get.mockResolvedValueOnce(doneSession);

    useChatStore.setState({
      activeSessionId: 's1',
      isStreaming: true,
      currentTurn: [makeEvent({ kind: 'started' }), makeEvent({ kind: 'text', delta: 'Hi!' })],
      sessions: [mockSession1],
    });

    useChatStore.getState().handleStreamEvent(makeEvent({ kind: 'done' }));

    // isStreaming cleared immediately on done
    // Wait for async reload
    await vi.waitFor(() => {
      const state = useChatStore.getState();
      expect(state.isStreaming).toBe(false);
      expect(state.currentTurn).toHaveLength(0);
      expect(state.sessions[0].messages).toHaveLength(2);
    });
  });
});
