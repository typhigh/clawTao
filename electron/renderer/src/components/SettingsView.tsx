import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useSettingsStore,
  LlmConfig, ProviderConfig,
  PROVIDER_TEMPLATES,
} from '../stores/settings';
import { LanguageSwitcher } from './LanguageSwitcher';
import { ProviderRow } from './ProviderRow';
import type { ModelOption } from './ChatView';

export function SettingsView({ onBack }: { onBack?: () => void }) {
  const { t } = useTranslation();
  const config = useSettingsStore(s => s.config);
  const savedConfig = useSettingsStore(s => s.savedConfig);
  const load = useSettingsStore(s => s.load);
  const save = useSettingsStore(s => s.save);
  const replace = useSettingsStore(s => s.replace);
  const removeProvider = useSettingsStore(s => s.removeProvider);

  useEffect(() => { load(); }, [load]);

  if (!config) return null;

  const handleAddProvider = (id: string) => {
    if (config.providers.some(p => p.id === id)) return;
    const tmpl = PROVIDER_TEMPLATES[id];
    if (!tmpl) return;
    const np: ProviderConfig = { id: tmpl.id, api_key: '', base_url: tmpl.base_url, api_protocol: tmpl.api_protocol, models: [] };
    replace({ ...config, providers: [...config.providers, np] });
  };

  const handleCancel = (id: string) => {
    const saved = savedConfig?.providers.find(p => p.id === id);
    if (saved) {
      replace({ ...config, providers: config.providers.map(p => p.id === id ? { ...saved, api_key: saved.api_key } : p) });
    } else {
      const remaining = config.providers.filter(p => p.id !== id);
      replace({ ...config, providers: remaining });
    }
  };

  const handleSave = async (cfg: LlmConfig) => { await save(cfg); };

  const availableToAdd = Object.values(PROVIDER_TEMPLATES).filter(tmpl => !config.providers.some(p => p.id === tmpl.id));

  return (
    <div className="settings-view">
      <div className="settings-view-body">
        <div className="settings-section-label">{t('settings.providers')}</div>
        <ul className="settings-provider-rows">
          {config.providers.map(provider => {
            const tmpl = PROVIDER_TEMPLATES[provider.id];
            if (!tmpl) return null;
            const isNew = !savedConfig?.providers.some(p => p.id === provider.id);
            const saved = savedConfig?.providers.find(p => p.id === provider.id);
            const hasSavedKey = !isNew && !!(saved?.api_key);

            return (
              <ProviderRow
                key={provider.id}
                provider={provider}
                config={config}
                isNew={isNew}
                hasSavedKey={hasSavedKey}
                onUpdate={(patch) => replace({ ...config, providers: config.providers.map(p => p.id === provider.id ? { ...p, ...patch } : p) })}
                onCancel={() => handleCancel(provider.id)}
                onSave={() => handleSave(config)}
                onRemove={() => removeProvider(provider.id)}
              />
            );
          })}
        </ul>

        {availableToAdd.length > 0 && (
          <div className="settings-add-provider">
            <select value="" onChange={(e) => { if (e.target.value) handleAddProvider(e.target.value); }}>
              <option value="" disabled hidden>{t('settings.addProvider')}</option>
              {availableToAdd.map(tmpl => <option key={tmpl.id} value={tmpl.id}>{tmpl.name}</option>)}
            </select>
          </div>
        )}

        <DefaultModelSelector config={config} onChange={(next) => replace(next)} />

        <div className="settings-inline-group">
          <div className="settings-inline-row">
            <label className="settings-inline-label">{t('settings.logLevel.label')}</label>
            <span className="settings-inline-select-wrap">
              <select className="settings-inline-select" value={config.log_level} onChange={async (e) => {
                const next = { ...config, log_level: e.target.value };
                replace(next);
                try { await save(next); } catch { /* ignore */ }
              }}>
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">Info</option>
                <option value="debug">Debug</option>
                <option value="trace">Trace</option>
              </select>
            </span>
          </div>
          <div className="settings-inline-row">
            <label className="settings-inline-label">{t('settings.language.label')}</label>
            <LanguageSwitcher />
          </div>
        </div>
      </div>
    </div>
  );
}

function DefaultModelSelector({ config, onChange }: { config: LlmConfig; onChange: (next: LlmConfig) => void }) {
  const options: ModelOption[] = useMemo(() => {
    const opts: ModelOption[] = [];
    for (const p of config.providers) {
      for (const m of p.models) {
        opts.push({ providerId: p.id, providerName: p.id, model: m, key: `${p.id}/${m}` });
      }
    }
    return opts;
  }, [config.providers]);

  if (options.length === 0) return null;

  return (
    <div className="settings-inline-row">
      <label className="settings-inline-label">Default Model</label>
      <span className="settings-inline-select-wrap">
        <select
          className="settings-inline-select"
          value={config.default_model_id}
          onChange={(e) => onChange({ ...config, default_model_id: e.target.value })}
        >
          <option value="">(none)</option>
          {options.map(o => <option key={o.key} value={o.key}>{o.model}</option>)}
        </select>
      </span>
    </div>
  );
}
