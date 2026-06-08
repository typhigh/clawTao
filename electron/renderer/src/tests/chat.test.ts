import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock electronAPI
const mockSession = {
  list: vi.fn(),
  create: vi.fn(),
  get: vi.fn(),
};

vi.stubGlobal('window', {
  electronAPI: {
    chat: { send: vi.fn() },
    session: mockSession,
    config: { get: vi.fn(), set: vi.fn(), validate: vi.fn(), testKey: vi.fn() },
    onChatStarted: vi.fn(),
    onTextDelta: vi.fn(),
    onChatDone: vi.fn(),
    onToolStarted: vi.fn(),
    onToolResult: vi.fn(),
  },
});

import { useChatStore } from '../stores/chat';

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

describe('chat store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useChatStore.setState({
      sessions: [],
      activeSessionId: null,
      streamingText: '',
      isStreaming: false,
      currentRunId: null,
      runningTools: [],
      error: null,
    });
  });

  it('starts with empty state', () => {
    const state = useChatStore.getState();
    expect(state.sessions).toEqual([]);
    expect(state.activeSessionId).toBeNull();
    expect(state.isStreaming).toBe(false);
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

  it('handleTextDelta accumulates text for active session', () => {
    useChatStore.setState({ activeSessionId: 's1', streamingText: '' });

    useChatStore.getState().handleTextDelta({ sessionId: 's1', delta: 'Hello' });
    expect(useChatStore.getState().streamingText).toBe('Hello');

    useChatStore.getState().handleTextDelta({ sessionId: 's1', delta: ' World' });
    expect(useChatStore.getState().streamingText).toBe('Hello World');
  });

  it('handleTextDelta ignores non-active session', () => {
    useChatStore.setState({ activeSessionId: 's2', streamingText: 'before' });

    useChatStore.getState().handleTextDelta({ sessionId: 's1', delta: 'nope' });
    expect(useChatStore.getState().streamingText).toBe('before');
  });

  it('handleChatStarted clears streaming text and sets runId', () => {
    useChatStore.setState({ activeSessionId: 's1', streamingText: 'old' });

    useChatStore.getState().handleChatStarted({ sessionId: 's1', runId: 'r99' });
    expect(useChatStore.getState().streamingText).toBe('');
    expect(useChatStore.getState().currentRunId).toBe('r99');
  });

  it('handleChatDone adds streaming text as message and clears state', () => {
    useChatStore.setState({
      activeSessionId: 's1',
      streamingText: 'Done!',
      isStreaming: true,
      sessions: [mockSession1],
    });

    useChatStore.getState().handleChatDone();

    const state = useChatStore.getState();
    expect(state.isStreaming).toBe(false);
    expect(state.streamingText).toBe('');
    expect(state.sessions.find(s => s.id === 's1')!.messages.length).toBe(2);
    expect(state.sessions.find(s => s.id === 's1')!.messages[1].content).toBe('Done!');
  });

  it('handleToolStarted adds running tool', () => {
    useChatStore.getState().handleToolStarted({
      sessionId: 's1',
      runId: 'r1',
      toolCallId: 'tc1',
      toolName: 'Bash',
      toolInput: { command: 'ls' },
    });

    expect(useChatStore.getState().runningTools).toHaveLength(1);
    expect(useChatStore.getState().runningTools[0].toolName).toBe('Bash');
    expect(useChatStore.getState().runningTools[0].result).toBeNull();
  });

  it('handleToolResult updates tool result', () => {
    useChatStore.getState().handleToolStarted({
      sessionId: 's1', runId: 'r1', toolCallId: 'tc1',
      toolName: 'Bash', toolInput: { command: 'ls' },
    });
    useChatStore.getState().handleToolResult({
      sessionId: 's1', runId: 'r1', toolCallId: 'tc1',
      toolName: 'Bash', result: 'stdout:\nfile.txt',
    });

    const tool = useChatStore.getState().runningTools[0];
    expect(tool.result).toBe('stdout:\nfile.txt');
  });
});
