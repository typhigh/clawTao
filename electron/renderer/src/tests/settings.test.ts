import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock electronAPI before importing the store
const mockConfigApi = {
  get: vi.fn(),
  set: vi.fn(),
  validate: vi.fn(),
  testKey: vi.fn(),
};

vi.stubGlobal('window', {
  electronAPI: {
    config: mockConfigApi,
    chat: { send: vi.fn() },
    session: { list: vi.fn(), create: vi.fn(), get: vi.fn() },
    onChatStarted: vi.fn(),
    onTextDelta: vi.fn(),
    onChatDone: vi.fn(),
    onToolStarted: vi.fn(),
    onToolResult: vi.fn(),
  },
});

import { useSettingsStore } from '../stores/settings';

describe('settings store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({ config: null, loaded: false });
  });

  it('starts with null config and not loaded', () => {
    const state = useSettingsStore.getState();
    expect(state.config).toBeNull();
    expect(state.loaded).toBe(false);
  });

  it('loads config and sets loaded flag', async () => {
    const mockConfig = {
      provider: 'openai',
      api_key: 'sk-1**cdef',
      base_url: 'https://api.openai.com/v1',
      model: 'gpt-4o',
      log_level: 'info',
      bash_blocked_commands: ['rm -rf /'],
    };
    mockConfigApi.get.mockResolvedValueOnce(mockConfig);

    await useSettingsStore.getState().load();

    const state = useSettingsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.config).toEqual(mockConfig);
  });

  it('handles load error gracefully', async () => {
    mockConfigApi.get.mockRejectedValueOnce(new Error('Rust not ready'));

    await useSettingsStore.getState().load();

    const state = useSettingsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.config).toBeNull();
  });

  it('save calls config.set and reloads masked config', async () => {
    const newConfig = {
      provider: 'minimax',
      api_key: 'sk-real',
      base_url: 'https://api.minimaxi.com/v1',
      model: 'MiniMax-M3',
      log_level: 'debug',
      bash_blocked_commands: ['rm -rf /'],
    };
    const maskedConfig = { ...newConfig, api_key: 'sk-r****real' };

    mockConfigApi.set.mockResolvedValueOnce({ ok: true });
    mockConfigApi.get.mockResolvedValueOnce(maskedConfig);

    await useSettingsStore.getState().save(newConfig);

    expect(mockConfigApi.set).toHaveBeenCalledWith(newConfig);
    expect(useSettingsStore.getState().config?.api_key).toBe('sk-r****real');
  });

  it('validate returns ok', async () => {
    mockConfigApi.validate.mockResolvedValueOnce({ ok: true });

    const result = await useSettingsStore.getState().validate();
    expect(result.ok).toBe(true);
  });

  it('validate returns error', async () => {
    mockConfigApi.validate.mockResolvedValueOnce({ ok: false, error: '401: Unauthorized' });

    const result = await useSettingsStore.getState().validate();
    expect(result.ok).toBe(false);
    expect(result.error).toBe('401: Unauthorized');
  });

  it('testKey sends credentials and returns result', async () => {
    mockConfigApi.testKey.mockResolvedValueOnce({ ok: true });

    const result = await useSettingsStore.getState().testKey('sk-test', 'https://api.test.com/v1', 'gpt-4o');
    expect(mockConfigApi.testKey).toHaveBeenCalledWith({
      api_key: 'sk-test',
      base_url: 'https://api.test.com/v1',
      model: 'gpt-4o',
    });
    expect(result.ok).toBe(true);
  });
});
