import { useTranslation } from 'react-i18next';

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
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
  disabledModel?: boolean;
}

export function InputArea({
  value, onChange, onSend, disabled, textareaRef, onKeyDown,
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
        </div>
        <button type="submit" className="input-send-btn" disabled={!value.trim() || disabled}>{t('chat.send')}</button>
      </form>
    </div>
  );
}