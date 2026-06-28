/**
 * Settings Store — persisted LLM configuration.
 *
 * Single source of truth for all chat sessions. A user maintains a list of
 * providers, each with its own credentials and model list, plus an active
 * provider/model pair that the chat uses.
 */
import { create } from 'zustand';

export const DEFAULT_BASH_TIMEOUT_SECS = 600;

/** Built-in providers (name is fixed; user adds/removes models). */
export interface ProviderTemplate {
  id: string;
  name: string;
  base_url: string;
  api_protocol: 'anthropic' | 'openai';
  baseUrlLocked: boolean;
  protocolLocked: boolean;
}

export const PROVIDER_TEMPLATES: Record<string, ProviderTemplate> = {
  deepseek: { id: 'deepseek', name: 'DeepSeek', base_url: 'https://api.deepseek.com/anthropic', api_protocol: 'anthropic', baseUrlLocked: true, protocolLocked: true },
  minimax:  { id: 'minimax',  name: 'MiniMax',  base_url: 'https://api.minimaxi.com/anthropic', api_protocol: 'anthropic', baseUrlLocked: true, protocolLocked: true },
  custom:   { id: 'custom',   name: 'Custom',   base_url: '',                             api_protocol: 'anthropic', baseUrlLocked: false, protocolLocked: false },
};

export const SUGGESTED_MODELS: Record<string, string[]> = {
  deepseek: ['deepseek-v4-pro', 'deepseek-v4-flash'],
  minimax: ['MiniMax-M3'],
  custom: [],
};

/** A user-configured provider entry. */
export interface ProviderConfig {
  id: string;        // template id (deepseek/minimax/custom)
  api_key: string;
  base_url: string;
  api_protocol: 'anthropic' | 'openai';
  models: string[];  // user-added model names
}

export interface LlmConfig {
  providers: ProviderConfig[];
  active_provider_id: string;
  active_model_id: string;
  log_level: string;
  bash_blocked_commands: string[];
  bash_timeout_secs: number | null;
}

interface SettingsState {
  config: LlmConfig | null;     // current (possibly edited) state — UI source of truth
  savedConfig: LlmConfig | null; // last persisted version — used to revert dirty edits
  loaded: boolean;
  load: () => Promise<void>;
  save: (c: LlmConfig) => Promise<void>;
  replace: (c: LlmConfig) => void;
  removeProvider: (id: string) => Promise<void>;
}

/** Probe an LLM API endpoint via Electron main process (respects system proxy). */
export async function probeConnection(
  base_url: string, _model: string, api_key: string, api_protocol: string,
  provider_id?: string,
): Promise<{ ok: boolean; error?: string }> {
  return window.electronAPI.config.probe({ base_url, model: _model, api_key, api_protocol, provider_id: provider_id ?? null } as any) as Promise<{ ok: boolean; error?: string }>;
}

/** Build a fresh empty config — no providers until the user adds one. */
export function emptyConfig(): LlmConfig {
  return {
    providers: [],
    active_provider_id: '',
    active_model_id: '',
    log_level: 'info',
    bash_blocked_commands: [],
    bash_timeout_secs: DEFAULT_BASH_TIMEOUT_SECS,
  };
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  config: null,
  savedConfig: null,
  loaded: false,

  load: async () => {
    try {
      const config = await window.electronAPI.config.get() as unknown as LlmConfig;
      set({ config, savedConfig: config, loaded: true });
    } catch {
      const e = emptyConfig();
      set({ config: e, savedConfig: e, loaded: true });
    }
  },

  save: async (c: LlmConfig) => {
    // Backend (chat.send) reads top-level provider/base_url/api_protocol/model/api_key
    // from config.json. Resolve the active provider and project those fields onto
    // the top level so chat routing keeps working without IPC changes.
    const active = c.providers.find(p => p.id === c.active_provider_id);
    const topLevel: Record<string, unknown> = {
      ...c,
      provider: active?.id ?? c.active_provider_id,
      base_url: active?.base_url ?? '',
      api_protocol: active?.api_protocol ?? 'anthropic',
      model: c.active_model_id,
    };
    await window.electronAPI.config.set(topLevel);
    const config = await window.electronAPI.config.get() as unknown as LlmConfig;
    set({ config, savedConfig: config });
  },

  replace: (c: LlmConfig) => set({ config: c }),

  removeProvider: async (id: string) => {
    const cur = get().config;
    if (!cur) return;
    // Tell main to clear the removed provider's stored key (api_key=null signals
    // "remove this key"), then drop it from `providers` so the file no longer
    // references it.
    const withNullKey = {
      ...cur,
      providers: cur.providers.map(p => p.id === id ? { ...p, api_key: null } : p),
    };
    const withoutRemoved = {
      ...withNullKey,
      providers: withNullKey.providers.filter(p => p.id !== id),
    };
    const projected = withoutRemoved.providers.find(p => p.id === withoutRemoved.active_provider_id);
    const topLevel: Record<string, unknown> = {
      ...withoutRemoved,
      provider: projected?.id ?? withoutRemoved.active_provider_id,
      base_url: projected?.base_url ?? '',
      api_protocol: projected?.api_protocol ?? 'anthropic',
      model: withoutRemoved.active_model_id,
    };
    await window.electronAPI.config.set(topLevel);
    const fresh = await window.electronAPI.config.get() as unknown as LlmConfig;
    set({ config: fresh, savedConfig: fresh });
  },
}));