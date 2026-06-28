/**
 * Settings Store — persisted LLM configuration.
 */
import { create } from 'zustand';

export const DEFAULT_BASH_TIMEOUT_SECS = 600;

export const SUGGESTED_MODELS: Record<string, string[]> = {
  deepseek: ['deepseek-v4-pro', 'deepseek-v4-flash'],
  minimax: ['MiniMax-M3'],
  custom: [],
};

/** Probe an LLM API endpoint via Electron main process (respects system proxy). */
export async function probeConnection(
  base_url: string, _model: string, api_key: string, api_protocol: string,
): Promise<{ ok: boolean; error?: string }> {
  return window.electronAPI.config.probe({ base_url, model: _model, api_key, api_protocol }) as Promise<{ ok: boolean; error?: string }>;
}

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
  thinking_enabled: boolean;
}

interface SettingsState {
  config: LlmConfig | null;
  loaded: boolean;
  load: () => Promise<void>;
  save: (c: LlmConfig) => Promise<void>;
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
    const config = await window.electronAPI.config.get() as LlmConfig;
    set({ config });
  },
}));
