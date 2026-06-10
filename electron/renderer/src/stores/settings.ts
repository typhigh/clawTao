/**
 * Settings Store — persisted LLM configuration.
 */
import { create } from 'zustand';

export const DEFAULT_BASH_TIMEOUT_SECS = 600;

export const SUGGESTED_MODELS: Record<string, string[]> = {
  deepseek: ['deepseek-chat', 'deepseek-r1'],
  minimax: ['MiniMax-M3'],
  custom: [],
};

export interface LlmConfig {
  provider: string;
  api_key: string;
  base_url: string;
  model: string;
  api_protocol: string;
  log_level: string;
  models: string[];
  bash_blocked_commands: string[];
  bash_timeout_secs: number | null;
}

interface SettingsState {
  config: LlmConfig | null;
  loaded: boolean;
  load: () => Promise<void>;
  save: (c: LlmConfig) => Promise<void>;
  validate: () => Promise<{ ok: boolean; error?: string }>;
  testKey: (api_key: string, base_url: string, model: string, api_protocol: string) => Promise<{ ok: boolean; error?: string }>;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  config: null,
  loaded: false,

  load: async () => {
    try {
      const config = await window.electronAPI.config.get() as LlmConfig;
      set({ config, loaded: true });
    } catch {
      set({ loaded: true }); // Allow creating new config
    }
  },

  save: async (c: LlmConfig) => {
    await window.electronAPI.config.set(c);
    // Reload masked version
    const config = await window.electronAPI.config.get() as LlmConfig;
    set({ config });
  },

  validate: async () => {
    const result = await window.electronAPI.config.validate() as { ok: boolean; error?: string };
    return result;
  },

  testKey: async (api_key: string, base_url: string, model: string, api_protocol: string) => {
    const result = await window.electronAPI.config.testKey({ api_key, base_url, model, api_protocol }) as { ok: boolean; error?: string };
    return result;
  },
}));
