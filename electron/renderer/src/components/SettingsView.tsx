import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useSettingsStore,
  LlmConfig, ProviderConfig,
  PROVIDER_TEMPLATES,
  SUGGESTED_MODELS,
  emptyConfig,
  probeConnection,
} from '../stores/settings';
import { LanguageSwitcher } from './LanguageSwitcher';
import { TrashIcon } from './icons';

interface Props { onBack?: () => void; } // optional — kept to avoid breaking call sites

/** True if the currently edited row in `config` differs from `saved` for that row. */
function rowDirty(config: LlmConfig | null, saved: LlmConfig | null, providerId: string): boolean {
  if (!config || !saved) return false;
  const cur = config.providers.find(p => p.id === providerId);
  const sav = saved.providers.find(p => p.id === providerId);
  if (!cur && !sav) return false;
  if (!cur || !sav) return true;
  // api_key: an empty string in `cur` is treated as "no change" so the mask
  // stays valid; otherwise compare everything except api_key (we don't have
  // the plaintext in saved so we can't diff it here — that's tracked separately).
  if (cur.api_key !== '' && cur.api_key !== sav.api_key) return true;
  if (cur.base_url !== sav.base_url) return true;
  if (cur.api_protocol !== sav.api_protocol) return true;
  if (cur.models.length !== sav.models.length) return true;
  for (let i = 0; i < cur.models.length; i++) {
    if (cur.models[i] !== sav.models[i]) return true;
  }
  return false;
}

export function SettingsView(_props: Props = {}) {
  const { t } = useTranslation();
  const config = useSettingsStore(s => s.config);
  const savedConfig = useSettingsStore(s => s.savedConfig);
  const load = useSettingsStore(s => s.load);
  const save = useSettingsStore(s => s.save);
  const replace = useSettingsStore(s => s.replace);
  const removeProvider = useSettingsStore(s => s.removeProvider);

  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [editingKeyByProvider, setEditingKeyByProvider] = useState<Record<string, boolean>>({});
  type RowStatus = { status: 'idle' | 'ok' | 'error'; message: string };
  const [rowStatus, setRowStatus] = useState<Record<string, RowStatus>>({});
  const setStatus = (key: string, status: RowStatus['status'], message = '') =>
    setRowStatus(s => ({ ...s, [key]: { status, message } }));
  const [modelInputs, setModelInputs] = useState<Record<string, string>>({});

  useEffect(() => { load(); }, [load]);

  // Intentionally NO auto-expand: collapsed by default, the user opens rows
  // explicitly. Keeps the list compact for users with many providers.

  const updateProvider = (id: string, patch: Partial<ProviderConfig>) => {
    if (!config) return;
    replace({
      ...config,
      providers: config.providers.map(p => p.id === id ? { ...p, ...patch } : p),
    });
  };

  const handleAddProvider = (id: string) => {
    if (!config) return;
    if (config.providers.some(p => p.id === id)) return;
    const tmpl = PROVIDER_TEMPLATES[id];
    if (!tmpl) return;
    const newProvider: ProviderConfig = {
      id: tmpl.id, api_key: '', base_url: tmpl.base_url,
      api_protocol: tmpl.api_protocol, models: [],
    };
    replace({
      ...config,
      providers: [...config.providers, newProvider],
      active_provider_id: config.active_provider_id || tmpl.id,
    });
    setEditingKeyByProvider(s => ({ ...s, [tmpl.id]: true }));
    setExpandedId(tmpl.id);
  };

  const handleCancel = (id: string) => {
    if (!config || !savedConfig) return;
    setEditingKeyByProvider(s => { const c = { ...s }; delete c[id]; return c; });
    setModelInputs(s => { const c = { ...s }; delete c[id]; return c; });
    setRowStatus(s => { const c = { ...s }; delete c[id]; return c; });

    const saved = savedConfig.providers.find(p => p.id === id);
    if (saved) {
      // Edit existing: revert this row to last-saved state.
      replace({
        ...config,
        providers: config.providers.map(p =>
          p.id === id ? { ...saved, api_key: '' } : p,
        ),
        active_model_id: config.active_provider_id === id
          ? savedConfig.active_model_id
          : config.active_model_id,
      });
    } else {
      // Never-saved new provider: drop it.
      const remaining = config.providers.filter(p => p.id !== id);
      const activeChanged = config.active_provider_id === id;
      replace({
        ...config,
        providers: remaining,
        active_provider_id: activeChanged ? (remaining[0]?.id ?? '') : config.active_provider_id,
        active_model_id: activeChanged ? '' : config.active_model_id,
      });
      if (expandedId === id) setExpandedId(null);
    }
  };

  const handleAddModel = (providerId: string) => {
    if (!config) return;
    const m = (modelInputs[providerId] || '').trim();
    if (!m) return;
    const provider = config.providers.find(p => p.id === providerId);
    if (!provider) return;
    if (provider.models.includes(m)) {
      setModelInputs(s => ({ ...s, [providerId]: '' }));
      return;
    }
    const nextModels = [...provider.models, m];
    updateProvider(providerId, { models: nextModels });
    setModelInputs(s => ({ ...s, [providerId]: '' }));
    if (config.active_provider_id === providerId && !config.active_model_id) {
      replace({ ...config, active_model_id: m });
    }
  };

  const handleRemoveModel = (providerId: string, model: string) => {
    if (!config) return;
    const provider = config.providers.find(p => p.id === providerId);
    if (!provider) return;
    updateProvider(providerId, { models: provider.models.filter(m => m !== model) });
    if (config.active_model_id === model && config.active_provider_id === providerId) {
      replace({ ...config, active_model_id: '' });
    }
  };

  const handleTest = async (providerId: string) => {
    if (!config) return;
    const provider = config.providers.find(p => p.id === providerId);
    if (!provider) return;
    setStatus(providerId, 'idle');
    try {
      const result = await probeConnection(
        provider.base_url,
        config.active_model_id || '__probe__',
        provider.api_key,
        provider.api_protocol,
        providerId,
      );
      setStatus(providerId, result.ok ? 'ok' : 'error', result.error || t('settings.connectionSuccess'));
    } catch (e) { setStatus(providerId, 'error', String(e)); }
  };

  const handleSave = async (providerId: string) => {
    if (!config) return;
    const provider = config.providers.find(p => p.id === providerId);
    if (!provider) return;
    setStatus(providerId, 'idle');
    try {
      const result = await probeConnection(
        provider.base_url,
        config.active_model_id || '__probe__',
        provider.api_key,
        provider.api_protocol,
        providerId,
      );
      if (!result.ok) {
        setStatus(providerId, 'error', result.error || 'Validation failed');
        return;
      }
      await save(config);
      setStatus(providerId, 'ok', t('settings.providerSaved'));
    } catch (e) { setStatus(providerId, 'error', String(e)); }
  };

  

  if (!config) return null;

  const availableToAdd = Object.values(PROVIDER_TEMPLATES).filter(
    tmpl => !config.providers.some(p => p.id === tmpl.id),
  );

  return (
    <div className="settings-view">
      <div className="settings-view-body">
        <div className="settings-section-label">{t('settings.providers')}</div>
        <ul className="settings-provider-rows">
          {config.providers.map(provider => {
            const tmpl = PROVIDER_TEMPLATES[provider.id];
            if (!tmpl) return null;
            const isOpen = expandedId === provider.id;
            const editingKey = !!editingKeyByProvider[provider.id];
            const isNew = !savedConfig?.providers.some(p => p.id === provider.id);
            const hasSavedKey = !isNew && !editingKey;
            const isDirty = rowDirty(config, savedConfig, provider.id);
            const suggested = SUGGESTED_MODELS[provider.id] || [];
            return (
              <li key={provider.id} className={`settings-provider-row ${isOpen ? 'open' : ''}`}>
                <div className="settings-provider-row-header">
                  <button
                    type="button"
                    className="settings-provider-row-toggle"
                    onClick={() => setExpandedId(isOpen ? null : provider.id)}
                    aria-expanded={isOpen}
                  >
                    <span className="settings-row-chevron">›</span>
                    <span className="settings-row-name">{tmpl.name}</span>
                    {provider.models.length > 0 && (
                      <span className="settings-row-meta-inline">
                        {t('settings.modelCount', { count: provider.models.length })}
                      </span>
                    )}
                    {isDirty && <span className="settings-row-dirty" title={t('settings.unsaved')} />}
                  </button>
                  <button
                    type="button"
                    className="settings-provider-row-delete"
                    onClick={(e) => { e.stopPropagation(); removeProvider(provider.id); }}
                    title={t('settings.removeProvider')}
                    aria-label={t('settings.removeProvider')}
                  ><TrashIcon /></button>
                </div>

                {isOpen && (
                  <div className="settings-provider-row-body">
                    <div className="form-group">
                      <label>{t('settings.apiKey')}</label>
                      {hasSavedKey ? (
                        <div className="settings-key-row">
                          <code>{provider.api_key}</code>
                          <button className="btn btn-secondary" onClick={() => {
                            setEditingKeyByProvider(s => ({ ...s, [provider.id]: true }));
                            updateProvider(provider.id, { api_key: '' });
                          }}>{t('settings.change')}</button>
                        </div>
                      ) : (
                        <div className="settings-key-row">
                          <input
                            type="password"
                            value={provider.api_key}
                            onChange={(e) => { updateProvider(provider.id, { api_key: e.target.value }); setStatus(provider.id, 'idle'); }}
                            placeholder="sk-..."
                          />
                          {!isNew && editingKey && (
                            <button className="btn btn-secondary" onClick={() => {
                              setEditingKeyByProvider(s => ({ ...s, [provider.id]: false }));
                              // Revert the in-progress edit; the saved mask is restored by
                              // dropping this field back to '' and letting the next read
                              // fall back to saved state on save. To keep it immediate, we
                              // also pull the saved api_key mask back into the live config.
                              const savedKey = savedConfig?.providers.find(p => p.id === provider.id)?.api_key ?? '';
                              updateProvider(provider.id, { api_key: savedKey });
                            }}>{t('settings.cancelEdit')}</button>
                          )}
                        </div>
                      )}
                    </div>

                    {!tmpl.baseUrlLocked && (
                      <div className="form-group">
                        <label>{t('settings.baseUrl')}</label>
                        <input
                          type="text"
                          value={provider.base_url}
                          onChange={(e) => updateProvider(provider.id, { base_url: e.target.value })}
                        />
                      </div>
                    )}

                    {!tmpl.protocolLocked && (
                      <div className="form-group">
                        <label>{t('settings.apiProtocol')}</label>
                        <select
                          value={provider.api_protocol}
                          onChange={(e) => updateProvider(provider.id, { api_protocol: e.target.value as 'anthropic' | 'openai' })}
                        >
                          <option value="openai">OpenAI</option>
                          <option value="anthropic">Anthropic</option>
                        </select>
                      </div>
                    )}

                    <div className="form-group">
                      <label>{t('settings.models')}</label>
                      {provider.models.length > 0 && (
                        <ul className="settings-model-list">
                          {provider.models.map(m => (
                            <li key={m}>
                              <span>{m}</span>
                              <button className="settings-model-remove" onClick={() => handleRemoveModel(provider.id, m)} title={t('settings.removeModel')}>×</button>
                            </li>
                          ))}
                        </ul>
                      )}
                      <div className="settings-model-add-row">
                        <input
                          type="text"
                          className="settings-model-input"
                          value={modelInputs[provider.id] || ''}
                          onChange={(e) => setModelInputs(s => ({ ...s, [provider.id]: e.target.value }))}
                          onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); handleAddModel(provider.id); } }}
                          placeholder={t('settings.enterModelId')}
                        />
                        <button className="settings-add-btn" onClick={() => handleAddModel(provider.id)} title={t('settings.addModel')}>+</button>
                        {suggested.length > 0 && (
                          <select
                            className="settings-model-suggest"
                            value=""
                            onChange={(e) => { if (e.target.value) { setModelInputs(s => ({ ...s, [provider.id]: e.target.value })); } }}
                            title={t('settings.suggested')}
                          >
                            <option value="">{t('settings.suggested')}</option>
                            {suggested
                              .filter(m => !provider.models.includes(m))
                              .map(m => <option key={m} value={m}>{m}</option>)}
                          </select>
                        )}
                      </div>
                      {provider.models.length === 0 && (
                        <div className="settings-hint">{t('settings.noModels')}</div>
                      )}
                    </div>

                    <div className="settings-row-actions">
                      <div className="settings-row-status">
                        {rowStatus[provider.id]?.status === 'ok' && (
                          <span className="settings-row-status-ok">{rowStatus[provider.id]?.message}</span>
                        )}
                        {rowStatus[provider.id]?.status === 'error' && (
                          <span className="settings-row-status-error">{rowStatus[provider.id]?.message}</span>
                        )}
                      </div>
                      <div className="settings-row-buttons">
                        <button
                          className="btn btn-secondary"
                          onClick={() => handleTest(provider.id)}
                        >{t('settings.testConnection')}</button>
                        <button
                          className="btn btn-primary"
                          disabled={!isDirty}
                          onClick={() => handleSave(provider.id)}
                        >{t('settings.save')}</button>
                        <button
                          className="btn btn-danger-text"
                          disabled={!isDirty}
                          onClick={() => handleCancel(provider.id)}
                        >{t('settings.cancelEdit')}</button>
                      </div>
                    </div>
                  </div>
                )}
              </li>
            );
          })}
        </ul>

        {availableToAdd.length > 0 && (
          <div className="settings-add-provider">
            <select
              value=""
              onChange={(e) => { if (e.target.value) handleAddProvider(e.target.value); }}
            >
              <option value="" disabled hidden>{t('settings.addProvider')}</option>
              {availableToAdd.map(tmpl => (
                <option key={tmpl.id} value={tmpl.id}>{tmpl.name}</option>
              ))}
            </select>
          </div>
        )}

        <div className="settings-inline-group">
          <div className="settings-inline-row">
            <label className="settings-inline-label">{t('settings.logLevel.label')}</label>
            <span className="settings-inline-select-wrap">
              <select
                className="settings-inline-select"
                value={config.log_level}
                onChange={async (e) => {
                  const next = { ...config, log_level: e.target.value };
                  replace(next);
                  try { await save(next); } catch { /* ignore */ }
                }}
              >
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

// Avoid unused-import warnings in some bundlers.
void emptyConfig;
