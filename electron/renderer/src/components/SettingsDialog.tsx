import { useState, useEffect } from 'react';
import { useSettingsStore, LlmConfig, DEFAULT_BASH_TIMEOUT_SECS, SUGGESTED_MODELS } from '../stores/settings';

interface Props { open: boolean; onClose: () => void; }

const PROVIDER_DEFAULTS: Record<string, { base_url: string; protocol: string; baseUrlLocked: boolean; protocolLocked: boolean }> = {
  deepseek: { base_url: 'https://api.deepseek.com/v1', protocol: 'openai', baseUrlLocked: true, protocolLocked: true },
  minimax:  { base_url: 'https://api.minimaxi.com/v1',  protocol: 'openai', baseUrlLocked: true, protocolLocked: true },
  custom:   { base_url: '',  protocol: 'openai', baseUrlLocked: false, protocolLocked: false },
};

function emptyConfig(): LlmConfig {
  return { provider: 'deepseek', api_key: '', base_url: PROVIDER_DEFAULTS.deepseek.base_url, model: '', models: [], api_protocol: 'openai', log_level: 'info', bash_blocked_commands: [], bash_timeout_secs: DEFAULT_BASH_TIMEOUT_SECS as any };
}

export function SettingsDialog({ open, onClose }: Props) {
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
          <h2>⚙️ LLM Provider Settings</h2>
          <button className="dialog-close" onClick={onClose}>×</button>
        </div>

        <div className="dialog-body">
          <div className="form-group">
            <label>Provider</label>
            <select value={tmp.provider} onChange={(e) => handleProviderChange(e.target.value)}>
              <option value="deepseek">DeepSeek</option>
              <option value="minimax">MiniMax</option>
              <option value="custom">Custom</option>
            </select>
          </div>

          <div className="form-group">
            <label>API Key</label>
            {hasSavedKey ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <code style={{ flex: 1, padding: '6px 12px', background: '#f5f5f5', borderRadius: 8, fontSize: 13 }}>{config!.api_key}</code>
                <button className="btn btn-secondary" onClick={() => { setEditingKey(true); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>Change</button>
              </div>
            ) : (
              <div style={{ display: 'flex', gap: 8 }}>
                <input type="password" value={tmp.api_key} onChange={(e) => { setTmpField('api_key', e.target.value); setTestResult('idle'); }} placeholder={editingKey ? 'Enter new API key...' : 'sk-...'} style={{ flex: 1, padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 14 }} />
                {editingKey && <button className="btn btn-secondary" onClick={() => { setEditingKey(false); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>Cancel</button>}
              </div>
            )}
          </div>

          <div className="form-group">
            <label>Base URL {providerDefaults.baseUrlLocked ? '(auto)' : ''}</label>
            <input type="text" value={tmp.base_url} onChange={(e) => setTmpField('base_url', e.target.value)} disabled={providerDefaults.baseUrlLocked} />
          </div>

          {tmp.provider === 'custom' && (
            <div className="form-group">
              <label>API Protocol</label>
              <select value={(tmp as any).api_protocol || 'openai'} onChange={(e) => setTmpField('api_protocol' as any, e.target.value)}>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </div>
          )}

          <div className="form-group">
            <label>Model</label>
            <div style={{ display: 'flex', gap: 8 }}>
              <input type="text" value={tmp.model} onChange={(e) => setTmpField('model', e.target.value)} placeholder="Enter model ID" style={{ flex: 1 }} />
              {SUGGESTED_MODELS[tmp.provider]?.length > 0 && (
                <select onChange={(e) => { if (e.target.value) setTmpField('model', e.target.value); }} style={{ width: 150 }}>
                  <option value="">Suggested</option>
                  {SUGGESTED_MODELS[tmp.provider].map(m => <option key={m} value={m}>{m}</option>)}
                </select>
              )}
            </div>
          </div>

          <div className="form-group">
            <label>Log Level</label>
            <select value={tmp.log_level} onChange={(e) => setTmpField('log_level', e.target.value)}>
              <option value="error">Error</option>
              <option value="warn">Warn</option>
              <option value="info">Info</option>
              <option value="debug">Debug</option>
              <option value="trace">Trace</option>
            </select>
          </div>

          <div className="form-group">
            <label>Bash Blocked Commands (one per line)</label>
            <textarea rows={5} value={tmp.bash_blocked_commands.join('\n')} onChange={(e) => setTmpField('bash_blocked_commands', e.target.value.split('\n').map(s => s.trim()).filter(Boolean))} style={{ width: '100%', padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 12, fontFamily: 'monospace', resize: 'vertical' }} />
          </div>

          <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <label style={{ margin: 0 }}>Bash Timeout</label>
            <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13, cursor: 'pointer', margin: 0 }}>
              <input type="checkbox" checked={tmp.bash_timeout_secs !== null} onChange={(e) => setTmpField('bash_timeout_secs' as any, e.target.checked ? DEFAULT_BASH_TIMEOUT_SECS : null)} />
              Enable
            </label>
            {tmp.bash_timeout_secs !== null && (
              <>
                <input type="number" value={tmp.bash_timeout_secs} onChange={(e) => setTmpField('bash_timeout_secs' as any, parseInt(e.target.value) || DEFAULT_BASH_TIMEOUT_SECS)} style={{ width: 80, padding: '4px 8px', border: '1px solid #ddd', borderRadius: 6, fontSize: 13 }} />
                <span style={{ fontSize: 12, color: '#999' }}>seconds</span>
              </>
            )}
          </div>

          {testResult === 'ok' && <div className="alert alert-success">✅ Connection successful</div>}
          {testResult === 'error' && <div className="alert alert-error">{testMessage}</div>}
        </div>

        <div className="dialog-footer">
          <button className="btn btn-secondary" onClick={handleTest}>Test Connection</button>
          <button className="btn btn-primary" onClick={handleSave}>Save</button>
        </div>
      </div>
    </div>
  );
}
