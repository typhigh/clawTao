import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore, LlmConfig, DEFAULT_BASH_TIMEOUT_SECS, SUGGESTED_MODELS } from '../stores/settings';
import { LanguageSwitcher } from './LanguageSwitcher';

interface Props { open: boolean; onClose: () => void; }

const PROVIDER_DEFAULTS: Record<string, { base_url: string; protocol: string; baseUrlLocked: boolean; protocolLocked: boolean }> = {
  deepseek: { base_url: 'https://api.deepseek.com/anthropic', protocol: 'anthropic', baseUrlLocked: true, protocolLocked: true },
  minimax:  { base_url: 'https://api.minimaxi.com/anthropic', protocol: 'anthropic', baseUrlLocked: true, protocolLocked: true },
  custom:   { base_url: '',  protocol: 'anthropic', baseUrlLocked: false, protocolLocked: false },
};

function emptyConfig(): LlmConfig {
  return { provider: 'deepseek', api_key: '', base_url: PROVIDER_DEFAULTS.deepseek.base_url, model: '', models: [], api_protocol: 'openai', log_level: 'info', bash_blocked_commands: [], bash_timeout_secs: DEFAULT_BASH_TIMEOUT_SECS as any, thinking_enabled: true };
}

export function SettingsDialog({ open, onClose }: Props) {
  const { t } = useTranslation();
  const { config, load, save, testKey } = useSettingsStore();
  const [tmp, setTmp] = useState<LlmConfig>(emptyConfig);
  const [editingKey, setEditingKey] = useState(false);
  const [testResult, setTestResult] = useState<'idle' | 'ok' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');

  useEffect(() => { if (open) { load(); setEditingKey(false); setTestResult('idle'); setTmp(emptyConfig()); } }, [open]);

  useEffect(() => {
    if (open && config) {
      setTmp({
        provider: config.provider || 'deepseek',
        api_key: '',
        base_url: config.base_url || PROVIDER_DEFAULTS[config.provider || 'deepseek']?.base_url || '',
        model: config.model || '',
        api_protocol: (config as any).api_protocol || 'openai',
        models: config.models || [config.model].filter(Boolean),
        log_level: config.log_level || 'info',
        bash_blocked_commands: config.bash_blocked_commands || [],
        bash_timeout_secs: (config as any).bash_timeout_secs ?? DEFAULT_BASH_TIMEOUT_SECS as any,
        thinking_enabled: (config as any).thinking_enabled ?? true,
      });
    }
  }, [open, config]);

  const setTmpField = <K extends keyof LlmConfig>(key: K, value: LlmConfig[K]) => {
    setTmp(prev => ({ ...prev, [key]: value }));
  };

  const handleProviderChange = (p: string) => {
    const d = PROVIDER_DEFAULTS[p];
    const updates: any = { provider: p };
    if (d) {
      updates.base_url = d.base_url;
      if (d.protocolLocked) updates.api_protocol = d.protocol;
    }
    setTmp(prev => ({ ...prev, ...updates }));
  };

  const handleTest = async () => {
    setTestResult('idle');
    try {
      const result = await testKey(tmp.api_key, tmp.base_url, tmp.model, (tmp as any).api_protocol || 'openai');
      setTestResult(result.ok ? 'ok' : 'error');
      setTestMessage(result.error || 'OK');
    } catch (e) { setTestResult('error'); setTestMessage(String(e)); }
  };

  const handleSave = async () => {
    setTestResult('idle');
    try {
      const result = await testKey(tmp.api_key, tmp.base_url, tmp.model, (tmp as any).api_protocol || 'openai');
      if (!result.ok) { setTestResult('error'); setTestMessage(result.error || 'Validation failed'); return; }
      await save(tmp);
      onClose();
    } catch (e) { setTestResult('error'); setTestMessage(String(e)); }
  };

  if (!open) return null;

  const providerDefaults = PROVIDER_DEFAULTS[tmp.provider] || PROVIDER_DEFAULTS.custom;
  const hasSavedKey = config?.api_key && !editingKey;

  return (
    <div className="dialog-overlay" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 480, maxHeight: '80vh', overflowY: 'auto', overflowX: 'hidden' }}>
        <div className="dialog-header">
          <h2>{t('settings.title')}</h2>
          <button className="dialog-close" onClick={onClose}>×</button>
        </div>

        <div className="dialog-body">
          <div className="form-group">
            <label>{t('settings.provider')}</label>
            <select value={tmp.provider} onChange={(e) => handleProviderChange(e.target.value)}>
              <option value="deepseek">DeepSeek</option>
              <option value="minimax">MiniMax</option>
              <option value="custom">Custom</option>
            </select>
          </div>

          <div className="form-group">
            <label>{t('settings.apiKey')}</label>
            {hasSavedKey ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <code style={{ flex: 1, padding: '6px 12px', background: '#f5f5f5', borderRadius: 8, fontSize: 13 }}>{config!.api_key}</code>
                <button className="btn btn-secondary" onClick={() => { setEditingKey(true); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>{t('settings.change')}</button>
              </div>
            ) : (
              <div style={{ display: 'flex', gap: 8 }}>
                <input type="password" value={tmp.api_key} onChange={(e) => { setTmpField('api_key', e.target.value); setTestResult('idle'); }} placeholder={editingKey ? t('settings.enterNewKey') : 'sk-...'} style={{ flex: 1, padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 14 }} />
                {editingKey && <button className="btn btn-secondary" onClick={() => { setEditingKey(false); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>{t('settings.cancel')}</button>}
              </div>
            )}
          </div>

          <div className="form-group">
            <label>{t('settings.baseUrl')} {providerDefaults.baseUrlLocked ? t('common.auto') : ''}</label>
            <input type="text" value={tmp.base_url} onChange={(e) => setTmpField('base_url', e.target.value)} disabled={providerDefaults.baseUrlLocked} />
          </div>

          {tmp.provider === 'custom' && (
            <div className="form-group">
              <label>{t('settings.apiProtocol')}</label>
              <select value={(tmp as any).api_protocol || 'openai'} onChange={(e) => setTmpField('api_protocol' as any, e.target.value)}>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </div>
          )}

          <div className="form-group">
            <label>{t('settings.model')}</label>
            <div style={{ display: 'flex', gap: 8 }}>
              <input type="text" value={tmp.model} onChange={(e) => setTmpField('model', e.target.value)} placeholder={t('settings.enterModelId')} style={{ flex: 1 }} />
              {SUGGESTED_MODELS[tmp.provider]?.length > 0 && (
                <select onChange={(e) => { if (e.target.value) setTmpField('model', e.target.value); }} style={{ width: 150 }}>
                  <option value="">{t('settings.suggested')}</option>
                  {SUGGESTED_MODELS[tmp.provider].map(m => <option key={m} value={m}>{m}</option>)}
                </select>
              )}
            </div>
          </div>

          <div className="form-group">
            <label>{t('settings.logLevel')}</label>
            <select value={tmp.log_level} onChange={(e) => setTmpField('log_level', e.target.value)}>
              <option value="error">Error</option>
              <option value="warn">Warn</option>
              <option value="info">Info</option>
              <option value="debug">Debug</option>
              <option value="trace">Trace</option>
            </select>
          </div>

          <div className="form-group">
            <label style={{ display: 'flex', alignItems: 'center', gap: 8, textTransform: 'none', fontWeight: 400, fontSize: 14 }}>
              <input
                type="checkbox"
                checked={tmp.thinking_enabled}
                onChange={(e) => setTmpField('thinking_enabled' as any, e.target.checked)}
              />
              {t('settings.thinkingEnabled')}
            </label>
          </div>

          <div className="form-group">
            <label>{t('settings.bashBlockedCommands')}</label>
            <textarea rows={5} value={tmp.bash_blocked_commands.join('\n')} onChange={(e) => setTmpField('bash_blocked_commands', e.target.value.split('\n').map(s => s.trim()).filter(Boolean))} style={{ width: '100%', padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 12, fontFamily: 'monospace', resize: 'vertical' }} />
          </div>

          <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <label style={{ margin: 0 }}>{t('settings.bashTimeout')}</label>
            <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13, cursor: 'pointer', margin: 0, whiteSpace: 'nowrap' }}>
              <input type="checkbox" checked={tmp.bash_timeout_secs !== null} onChange={(e) => setTmpField('bash_timeout_secs' as any, e.target.checked ? DEFAULT_BASH_TIMEOUT_SECS : null)} />
              {t('settings.enable')}
            </label>
            {tmp.bash_timeout_secs !== null && (
              <>
                <input type="number" value={tmp.bash_timeout_secs} onChange={(e) => setTmpField('bash_timeout_secs' as any, parseInt(e.target.value) || DEFAULT_BASH_TIMEOUT_SECS)} style={{ width: 80, padding: '4px 8px', border: '1px solid #ddd', borderRadius: 6, fontSize: 13 }} />
                <span style={{ fontSize: 12, color: '#999' }}>{t('settings.seconds')}</span>
              </>
            )}
          </div>

          {testResult === 'ok' && <div className="alert alert-success">{t('settings.connectionSuccess')}</div>}
          {testResult === 'error' && <div className="alert alert-error">{testMessage}</div>}
        </div>

        <div className="dialog-footer">
          <LanguageSwitcher />
          <div style={{ flex: 1 }} />
          <button className="btn btn-secondary" onClick={handleTest}>{t('settings.testConnection')}</button>
          <button className="btn btn-primary" onClick={handleSave}>{t('settings.save')}</button>
        </div>
      </div>
    </div>
  );
}
