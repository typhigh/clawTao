import { useTranslation } from 'react-i18next';
import { SendIcon } from './icons';

interface ModelOption {
  providerId: string;
  providerName: string;
  model: string;
  key: string; // "providerId/model"
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  disabled: boolean;
  sendDisabled?: boolean;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
  disabledModel?: boolean;
}

export function InputArea({
  value, onChange, onSend, disabled, sendDisabled, textareaRef, onKeyDown,
  modelOptions, selectedModelKey, onSelectModel, disabledModel,
}: Props) {
  const { t } = useTranslation();

  return (
    <div className="input-area-wrapper">
      <form className="input-area" onSubmit={(e) => { e.preventDefault(); onSend(); }}>
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
          <button
            type="submit"
            className="input-send-btn input-send-btn-inline"
            disabled={!value.trim() || disabled || sendDisabled}
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
              cursor: !value.trim() || disabled || sendDisabled ? 'not-allowed' : 'pointer',
              opacity: !value.trim() || disabled || sendDisabled ? 0.55 : 1,
            }}
          >
            <SendIcon />
          </button>
        </div>
      </form>
    </div>
  );
}