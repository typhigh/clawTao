/**
 * Settings Store — persisted LLM configuration.
 */
import { create } from 'zustand';

export interface LlmConfig {
  provider: string;
  api_key: string;
  base_url: string;
  model: string;
  log_level: string;
}

interface SettingsState {
  config: LlmConfig | null;
  loaded: boolean;
  load: () => Promise<void>;
  save: (c: LlmConfig) => Promise<void>;
  validate: () => Promise<{ ok: boolean; error?: string }>;
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
}));
