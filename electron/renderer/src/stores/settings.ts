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
  default_model_id: string;
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

/** Ensure default_model_id references a valid model. If not, pick the first available. */
function ensureDefaultModel(c: LlmConfig): LlmConfig {
  if (c.default_model_id) {
    const [pid, ...rest] = c.default_model_id.split('/');
    const m = rest.join('/');
    const provider = c.providers.find(p => p.id === pid);
    if (provider && provider.models.includes(m)) return c; // still valid
  }
  const first = c.providers.find(p => p.models.length > 0);
  return { ...c, default_model_id: first ? `${first.id}/${first.models[0]}` : '' };
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
    default_model_id: '',
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
    // Ensure default_model_id is set if providers have models.
    if (!c.default_model_id) {
      const firstWithModel = c.providers.find(p => p.models.length > 0);
      if (firstWithModel) c.default_model_id = `${firstWithModel.id}/${firstWithModel.models[0]}`;
    }
    await window.electronAPI.config.set(c as unknown as Record<string, unknown>);
    const config = await window.electronAPI.config.get() as unknown as LlmConfig;
    set({ config, savedConfig: config });
  },

  replace: (c: LlmConfig) => set({ config: ensureDefaultModel(c) }),

  removeProvider: async (id: string) => {
    const cur = get().config;
    if (!cur) return;
    // Drop the provider and fix default_model_id BEFORE null-ing api_key
    // (api_key: null is a signal to Electron to delete the stored key).
    const withoutRemoved = ensureDefaultModel({
      ...cur,
      providers: cur.providers.filter(p => p.id !== id),
    });
    const withNullKey = {
      ...withoutRemoved,
      providers: withoutRemoved.providers.map(p => p.id === id ? { ...p, api_key: null } : p),
    };
    await window.electronAPI.config.set(withNullKey as unknown as Record<string, unknown>);
    const fresh = await window.electronAPI.config.get() as unknown as LlmConfig;
    set({ config: fresh, savedConfig: fresh });
  },
}));