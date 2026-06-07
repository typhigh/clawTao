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

export function SettingsDialog({ open, onClose }: Props) {
  const { config, loaded, load, save, validate } = useSettingsStore();

  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [model, setModel] = useState('');
  const [provider, setProvider] = useState('openai');
  const [showKey, setShowKey] = useState(false);
  const [validating, setValidating] = useState(false);
  const [validationResult, setValidationResult] = useState<'idle' | 'ok' | 'error'>('idle');
  const [validationMessage, setValidationMessage] = useState('');
  const [saving, setSaving] = useState(false);
  const [logLevel, setLogLevel] = useState('info');

  useEffect(() => {
    if (open && !loaded) load();
  }, [open, loaded, load]);

  useEffect(() => {
    if (config) {
      // Don't pre-fill API key — config.get returns a masked value.
      // User must re-enter the real key each time they open settings.
      // Other fields are safe to pre-fill.
      setBaseUrl(config.base_url || PROVIDER_DEFAULTS.openai.base_url);
      setModel(config.model || 'gpt-4o');
      setProvider(config.provider || 'openai');
      setLogLevel(config.log_level || 'info');
    }
  }, [config]);

  const handleProviderChange = (p: string) => {
    setProvider(p);
    const defaults = PROVIDER_DEFAULTS[p];
    if (defaults) setBaseUrl(defaults.base_url);
  };

  const handleValidate = async () => {
    if (!apiKey) return;
    setValidating(true);
    setValidationResult('idle');

    // Save temp config first so Rust can validate it
    try {
      await save({ provider, api_key: apiKey, base_url: baseUrl, model, log_level: logLevel });
    } catch {
      setValidationResult('error');
      setValidationMessage('Failed to save config');
      setValidating(false);
      return;
    }

    try {
      const result = await validate();
      if (result.ok) {
        setValidationResult('ok');
      } else {
        setValidationResult('error');
        setValidationMessage(result.error || 'Validation failed');
      }
    } catch (e) {
      setValidationResult('error');
      setValidationMessage(String(e));
    }
    setValidating(false);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      // Only send api_key if user typed a new one; empty = keep existing
      const conf: any = { provider, base_url: baseUrl, model, log_level: logLevel };
      if (apiKey.trim()) conf.api_key = apiKey;
      await save(conf);
      onClose();
    } catch (e) {
      setValidationResult('error');
      setValidationMessage(String(e));
    }
    setSaving(false);
  };

  if (!open) return null;

  return (
    <div className="dialog-overlay" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 480 }}>
        <div className="dialog-header">
          <h2>⚙️ LLM Provider Settings</h2>
          <button className="dialog-close" onClick={onClose}>×</button>
        </div>

        <div className="dialog-body">
          {/* Provider */}
          <div className="form-group">
            <label>Provider</label>
            <select value={provider} onChange={(e) => handleProviderChange(e.target.value)}>
              {Object.keys(PROVIDER_DEFAULTS).map((p) => (
                <option key={p} value={p}>{p}</option>
              ))}
            </select>
          </div>

          {/* API Key */}
          <div className="form-group">
            <label>API Key</label>
            <div className="input-with-toggle">
              <input
                type={showKey ? 'text' : 'password'}
                value={apiKey}
                onChange={(e) => { setApiKey(e.target.value); setValidationResult('idle'); }}
                placeholder="sk-..."
              />
              <button
                type="button"
                className="toggle-btn"
                onClick={() => setShowKey(!showKey)}
                title={showKey ? 'Hide' : 'Show'}
              >
                {showKey ? '🙈' : '👁'}
              </button>
            </div>
          </div>

          {/* Base URL */}
          <div className="form-group">
            <label>Base URL</label>
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
            />
          </div>

          {/* Model */}
          <div className="form-group">
            <label>Model</label>
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="gpt-4o"
            />
          </div>

          {/* Log Level */}
          <div className="form-group">
            <label>Log Level</label>
            <select value={logLevel} onChange={(e) => setLogLevel(e.target.value)}>
              <option value="error">Error</option>
              <option value="warn">Warn</option>
              <option value="info">Info</option>
              <option value="debug">Debug</option>
              <option value="trace">Trace</option>
            </select>
          </div>

          {/* Validation status */}
          {validationResult === 'ok' && (
            <div className="alert alert-success">✅ Connection successful</div>
          )}
          {validationResult === 'error' && (
            <div className="alert alert-error">{validationMessage}</div>
          )}
        </div>

        <div className="dialog-footer">
          <button className="btn btn-secondary" onClick={handleValidate} disabled={!apiKey || validating}>
            {validating ? 'Testing...' : 'Test Connection'}
          </button>
          <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}
