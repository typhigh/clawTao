import { DEFAULT_BASH_TIMEOUT_SECS } from '../stores/settings';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock electronAPI before importing the store
const mockConfigApi = {
  get: vi.fn(),
  set: vi.fn(),
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

import { useSettingsStore, emptyConfig, PROVIDER_TEMPLATES } from '../stores/settings';

function makeConfig() {
  return {
    llm: {
      providers: [
        { id: 'deepseek', api_key: 'sk-ds**ef', base_url: PROVIDER_TEMPLATES.deepseek.base_url, api_protocol: 'anthropic' as const, models: ['deepseek-v4-pro'] },
        { id: 'minimax',  api_key: 'sk-mn**ef', base_url: PROVIDER_TEMPLATES.minimax.base_url,  api_protocol: 'anthropic' as const, models: ['MiniMax-M3'] },
      ],
      default_model_id: '',
    },
    log_level: 'info',
    bash: {
      blocked_commands: ['rm -rf /'],
      timeout_secs: DEFAULT_BASH_TIMEOUT_SECS,
    },
  };
}

describe('settings store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({ config: null, savedConfig: null, loaded: false });
  });

  it('starts with null config and not loaded', () => {
    const state = useSettingsStore.getState();
    expect(state.config).toBeNull();
    expect(state.loaded).toBe(false);
  });

  it('loads config and sets loaded flag', async () => {
    const mockConfig = makeConfig();
    mockConfigApi.get.mockResolvedValueOnce(mockConfig);

    await useSettingsStore.getState().load();

    const state = useSettingsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.config).toEqual(mockConfig);
  });

  it('preserves whatever shape main returns (no compatibility shim)', async () => {
    // Whatever main returns, the store just mirrors it. No fallback to empty.
    const raw = { provider: 'openai', api_key: 'sk-xxx', base_url: 'x', model: 'y' };
    mockConfigApi.get.mockResolvedValueOnce(raw);

    await useSettingsStore.getState().load();

    const state = useSettingsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.config).toEqual(raw);
    expect(state.savedConfig).toEqual(raw);
  });

  it('handles load error gracefully by returning empty config', async () => {
    mockConfigApi.get.mockRejectedValueOnce(new Error('Rust not ready'));

    await useSettingsStore.getState().load();

    const state = useSettingsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.config).toEqual(emptyConfig());
  });

  it('save calls config.set and reloads masked config', async () => {
    const newConfig = makeConfig();
    const maskedConfig = makeConfig();
    mockConfigApi.set.mockResolvedValueOnce({ ok: true });
    mockConfigApi.get.mockResolvedValueOnce(maskedConfig);

    await useSettingsStore.getState().save(newConfig);

    expect(mockConfigApi.set).toHaveBeenCalled();
    expect(useSettingsStore.getState().config?.llm.providers[0].api_key).toBe(maskedConfig.llm.providers[0].api_key);
  });

});