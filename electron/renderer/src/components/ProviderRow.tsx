import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppConfig, ProviderConfig } from '../stores/settings';
import {
  PROVIDER_TEMPLATES,
  SUGGESTED_MODELS,
  probeConnection,
  useSettingsStore,
} from '../stores/settings';
import { SuggestPicker } from './SuggestPicker';
import { TrashIcon } from './icons';

interface Props {
  provider: ProviderConfig;
  config: AppConfig;
  isNew: boolean;
  hasSavedKey: boolean;
  onUpdate: (patch: Partial<ProviderConfig>) => void;
  onCancel: () => void;
  onSave: () => Promise<void>;
  onRemove: () => void;
}

/// Detect unsaved changes. Does NOT check api_key — that's tracked separately
/// via the editingKey local state.
function rowDirty(cur: ProviderConfig, sav: ProviderConfig | undefined): boolean {
  if (!sav) return true; // new provider, never saved
  if (cur.base_url !== sav.base_url) return true;
  if (cur.api_protocol !== sav.api_protocol) return true;
  if (cur.models.length !== sav.models.length) return true;
  for (let i = 0; i < cur.models.length; i++) {
    if (cur.models[i] !== sav.models[i]) return true;
  }
  return false;
}

export function ProviderRow({ provider, isNew, hasSavedKey, onUpdate, onCancel, onSave, onRemove }: Props) {
  const { t } = useTranslation();
  const tmpl = PROVIDER_TEMPLATES[provider.id]!;
  const [open, setOpen] = useState(isNew);
  const [editingKey, setEditingKey] = useState(isNew);
  const [modelInput, setModelInput] = useState('');
  type RowStatus = { status: 'idle' | 'ok' | 'error'; message: string };
  const [rowStatus, setRowStatus] = useState<RowStatus>({ status: 'idle', message: '' });

  const savedConfig = useSettingsStore(s => s.savedConfig);
  const sav = savedConfig?.llm.providers.find(p => p.id === provider.id);
  const dirty = rowDirty(provider, sav) || editingKey;
  const suggested = SUGGESTED_MODELS[provider.id] || [];

  const handleTest = async () => {
    if (provider.models.length === 0) {
      setRowStatus({ status: 'error', message: t('settings.testRequiresModels') });
      return;
    }
    setRowStatus({ status: 'idle', message: '' });
    try {
      const result = await probeConnection(
        provider.base_url, provider.models[0], provider.api_key, provider.api_protocol, provider.id,
      );
      setRowStatus({ status: result.ok ? 'ok' : 'error', message: result.error || t('settings.connectionSuccess') });
    } catch (e) { setRowStatus({ status: 'error', message: String(e) }); }
  };

  const handleSave = async () => {
    if (provider.models.length === 0) {
      setRowStatus({ status: 'error', message: t('settings.testRequiresModels') });
      return;
    }
    setRowStatus({ status: 'idle', message: '' });
    try {
      const result = await probeConnection(
        provider.base_url, provider.models[0], provider.api_key, provider.api_protocol, provider.id,
      );
      if (!result.ok) { setRowStatus({ status: 'error', message: result.error || 'Validation failed' }); return; }
      await onSave();
      setRowStatus({ status: 'ok', message: t('settings.providerSaved') });
      setEditingKey(false);
    } catch (e) { setRowStatus({ status: 'error', message: String(e) }); }
  };

  const handleCancel = () => {
    setEditingKey(false);
    setModelInput('');
    setRowStatus({ status: 'idle', message: '' });
    onCancel();
  };

  const addModel = () => {
    const m = modelInput.trim();
    if (!m || provider.models.includes(m)) { setModelInput(''); return; }
    onUpdate({ models: [...provider.models, m] });
    setModelInput('');
  };

  return (
    <li className={`settings-provider-row ${open ? 'open' : ''}`}>
      <div className="settings-provider-row-header">
        <button type="button" className="settings-provider-row-toggle" onClick={() => setOpen(o => !o)} aria-expanded={open}>
          <span className="settings-row-chevron">›</span>
          <span className="settings-row-name">{tmpl.name}</span>
          {provider.models.length > 0 && (
            <span className="settings-row-meta-inline">{t('settings.modelCount', { count: provider.models.length })}</span>
          )}
          {dirty && <span className="settings-row-dirty" title={t('settings.unsaved')} />}
        </button>
        <button type="button" className="settings-provider-row-delete" onClick={(e) => { e.stopPropagation(); onRemove(); }} title={t('settings.removeProvider')} aria-label={t('settings.removeProvider')}><TrashIcon /></button>
      </div>

      {open && (
        <div className="settings-provider-row-body">
          <div className="form-group">
            <label>{t('settings.apiKey')}</label>
            {hasSavedKey && !editingKey ? (
              <div className="settings-key-row">
                <code>{provider.api_key}</code>
                <button className="btn btn-secondary" onClick={() => { setEditingKey(true); onUpdate({ api_key: '' }); }}>{t('settings.change')}</button>
              </div>
            ) : (
              <div className="settings-key-row">
                <input type="password" value={provider.api_key} onChange={(e) => { onUpdate({ api_key: e.target.value }); setRowStatus({ status: 'idle', message: '' }); }} placeholder="sk-..." />
                {!isNew && editingKey && (
                  <button className="btn btn-secondary" onClick={handleCancel}>{t('settings.cancelEdit')}</button>
                )}
              </div>
            )}
          </div>

          {!tmpl.baseUrlLocked && (
            <div className="form-group">
              <label>{t('settings.baseUrl')}</label>
              <input type="text" value={provider.base_url} onChange={(e) => onUpdate({ base_url: e.target.value })} />
            </div>
          )}

          {!tmpl.protocolLocked && (
            <div className="form-group">
              <label>{t('settings.apiProtocol')}</label>
              <select value={provider.api_protocol} onChange={(e) => onUpdate({ api_protocol: e.target.value as 'anthropic' | 'openai' })}>
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
                  <li key={m}><span>{m}</span>
                    <button className="settings-model-remove" onClick={() => {
                      const next = provider.models.filter(x => x !== m);
                      onUpdate({ models: next });
                    }} title={t('settings.removeModel')}>×</button>
                  </li>
                ))}
              </ul>
            )}
            <div className="settings-model-add-row">
              <input type="text" className="settings-model-input" value={modelInput} onChange={(e) => setModelInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addModel(); } }}
                placeholder={t('settings.enterModelId')} />
              <button className="settings-add-btn" onClick={addModel} title={t('settings.addModel')}>+</button>
              {suggested.length > 0 && (
                <SuggestPicker suggested={suggested.filter(m => !provider.models.includes(m))} placeholder={t('settings.suggested')} onPick={(m) => setModelInput(m)} />
              )}
            </div>
            {provider.models.length === 0 && <div className="settings-hint">{t('settings.noModels')}</div>}
          </div>

          <div className="settings-row-actions">
            <div className="settings-row-status">
              {rowStatus.status === 'ok' && <span className="settings-row-status-ok">{rowStatus.message}</span>}
              {rowStatus.status === 'error' && <span className="settings-row-status-error">{rowStatus.message}</span>}
            </div>
            <div className="settings-row-buttons">
              <button className="btn btn-secondary" onClick={handleTest}>{t('settings.testConnection')}</button>
              <button className="btn btn-primary" disabled={!dirty} onClick={handleSave}>{t('settings.save')}</button>
              {dirty && (
                <button className="btn btn-danger-text" onClick={handleCancel}>{t('settings.cancelEdit')}</button>
              )}
            </div>
          </div>
        </div>
      )}
    </li>
  );
}
