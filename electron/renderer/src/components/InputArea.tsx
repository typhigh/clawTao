import { useTranslation } from 'react-i18next';
import { SendIcon } from './icons';

interface ModelOption {
  providerId: string;
  providerName: string;
  model: string;
  key: string;
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  onCancel: () => void;
  disabled: boolean;
  streaming: boolean;
  sendDisabled?: boolean;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
  disabledModel?: boolean;
}

export function InputArea({
  value, onChange, onSend, onCancel, disabled, streaming, sendDisabled, textareaRef, onKeyDown,
  modelOptions, selectedModelKey, onSelectModel, disabledModel,
}: Props) {
  const { t } = useTranslation();

  const btnDisabled = !value.trim() || disabled || sendDisabled;

  return (
    <div className="input-area-wrapper">
      <form className="input-area" onSubmit={(e) => { e.preventDefault(); if (streaming) onCancel(); else onSend(); }}>
        <div className="input-textarea-wrap">
          <textarea
            ref={textareaRef}
            rows={3}
            placeholder={t('chat.placeholder')}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onKeyDown}
            disabled={disabled}
          />
          <select
            className="input-model-select input-model-select-inline"
            value={selectedModelKey}
            onChange={(e) => onSelectModel(e.target.value)}
            disabled={disabledModel || modelOptions.length === 0}
            title={t('chat.selectModel')}
          >
            {modelOptions.length === 0 ? (
              <option value="">{t('chat.noModelsConfigured')}</option>
            ) : (
              modelOptions.map(opt => (
                <option key={opt.key} value={opt.key}>
                  {opt.providerId === 'custom' ? `${opt.providerName} / ${opt.model}` : opt.model}
                </option>
              ))
            )}
          </select>
          {streaming ? (
            <button
              type="submit"
              className="input-send-btn input-send-btn-inline input-stop-btn"
              title={t('chat.stop')}
              style={{
                background: '#e53935',
                color: '#ffffff',
                border: 'none',
                borderRadius: '50%',
                width: '28px',
                height: '28px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                cursor: 'pointer',
              }}
            >
              <StopIcon />
            </button>
          ) : (
            <button
              type="submit"
              className="input-send-btn input-send-btn-inline"
              disabled={btnDisabled}
              title={t('chat.send')}
              style={{
                background: '#1c1c1e',
                color: '#ffffff',
                border: 'none',
                borderRadius: '50%',
                width: '28px',
                height: '28px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                cursor: btnDisabled ? 'not-allowed' : 'pointer',
                opacity: btnDisabled ? 0.55 : 1,
              }}
            >
              <SendIcon />
            </button>
          )}
        </div>
      </form>
    </div>
  );
}

function StopIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="3" y="3" width="10" height="10" rx="2" fill="currentColor" />
    </svg>
  );
}
