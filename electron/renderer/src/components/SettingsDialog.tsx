import { useState, useEffect } from 'react';
import { useSettingsStore, LlmConfig } from '../stores/settings';

interface Props {
  open: boolean;
  onClose: () => void;
}

const PROVIDER_DEFAULTS: Record<string, { base_url: string }> = {
  openai: { base_url: 'https://api.openai.com/v1' },
  minimax: { base_url: 'https://api.minimaxi.com/v1' },
  groq: { base_url: 'https://api.groq.com/openai/v1' },
  deepseek: { base_url: 'https://api.deepseek.com/v1' },
  ollama: { base_url: 'http://localhost:11434/v1' },
  custom: { base_url: '' },
};

function emptyConfig(): LlmConfig {
  return { provider: 'openai', api_key: '', base_url: '', model: '', log_level: 'info', bash_blocked_commands: [] };
}

export function SettingsDialog({ open, onClose }: Props) {
  const { config, loaded, load, save, validate, testKey } = useSettingsStore();

  // tmpConfig is the editing copy. Initialize once when dialog opens.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const [tmp, setTmp] = useState<LlmConfig>(emptyConfig);
  const [editingKey, setEditingKey] = useState(false);
  const [testResult, setTestResult] = useState<'idle' | 'ok' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');

  // Initialize tmp from loaded config when dialog opens
  useEffect(() => {
    if (open) {
      load();
      setEditingKey(false);
      setTestResult('idle');
      setTmp(emptyConfig());
    }
  }, [open]);

  useEffect(() => {
    if (open && config) {
      setTmp({
        provider: config.provider || 'openai',
        api_key: '', // never pre-fill key
        base_url: config.base_url || PROVIDER_DEFAULTS.openai.base_url,
        model: config.model || 'gpt-4o',
        log_level: config.log_level || 'info',
        bash_blocked_commands: config.bash_blocked_commands || [],
      });
    }
  }, [open, config]);

  const setTmpField = <K extends keyof LlmConfig>(key: K, value: LlmConfig[K]) => {
    setTmp(prev => ({ ...prev, [key]: value }));
  };

  const handleTest = async () => {
    setTestResult('idle');
    try {
      if (tmp.api_key) {
        // User is typing a new key — test it directly
        const result = await testKey(tmp.api_key, tmp.base_url, tmp.model);
        setTestResult(result.ok ? 'ok' : 'error');
        setTestMessage(result.error || 'OK');
      } else {
        // Test currently saved key
        const result = await validate();
        setTestResult(result.ok ? 'ok' : 'error');
        setTestMessage(result.error || 'OK');
      }
    } catch (e) {
      setTestResult('error');
      setTestMessage(String(e));
    }
  };

  const handleSave = async () => {
    // Always validate before saving
    setTestResult('idle');
    try {
      const result = tmp.api_key
        ? await testKey(tmp.api_key, tmp.base_url, tmp.model)
        : await validate();
      if (!result.ok) {
        setTestResult('error');
        setTestMessage(result.error || 'Validation failed');
        return;
      }
      await save(tmp);
      onClose();
    } catch (e) {
      setTestResult('error');
      setTestMessage(String(e));
    }
  };

  if (!open) return null;

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
            <select value={tmp.provider} onChange={(e) => {
              setTmpField('provider', e.target.value);
              const d = PROVIDER_DEFAULTS[e.target.value];
              if (d) setTmpField('base_url', d.base_url);
            }}>
              {Object.keys(PROVIDER_DEFAULTS).map(p => <option key={p} value={p}>{p}</option>)}
            </select>
          </div>

          <div className="form-group">
            <label>API Key</label>
            {hasSavedKey ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <code style={{ flex: 1, padding: '6px 12px', background: '#f5f5f5', borderRadius: 8, fontSize: 13 }}>
                  {config!.api_key}
                </code>
                <button className="btn btn-secondary" onClick={() => { setEditingKey(true); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>
                  Change
                </button>
              </div>
            ) : (
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  type="password"
                  value={tmp.api_key}
                  onChange={(e) => { setTmpField('api_key', e.target.value); setTestResult('idle'); }}
                  placeholder={editingKey ? 'Enter new API key...' : 'sk-...'}
                  style={{ flex: 1, padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 14 }}
                />
                {editingKey && (
                  <button className="btn btn-secondary" onClick={() => { setEditingKey(false); setTmpField('api_key', ''); }} style={{ fontSize: 12 }}>
                    Cancel
                  </button>
                )}
              </div>
            )}
          </div>

          <div className="form-group">
            <label>Base URL</label>
            <input type="text" value={tmp.base_url} onChange={(e) => setTmpField('base_url', e.target.value)} />
          </div>

          <div className="form-group">
            <label>Model</label>
            <input type="text" value={tmp.model} onChange={(e) => setTmpField('model', e.target.value)} placeholder="gpt-4o" />
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
            <textarea
              rows={5}
              value={tmp.bash_blocked_commands.join('\n')}
              onChange={(e) => setTmpField('bash_blocked_commands', e.target.value.split('\n').map(s => s.trim()).filter(Boolean))}
              style={{ width: '100%', padding: '8px 12px', border: '1px solid #ddd', borderRadius: 8, fontSize: 12, fontFamily: 'monospace', resize: 'vertical' }}
            />
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
