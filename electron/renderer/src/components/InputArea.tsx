import { useTranslation } from 'react-i18next';

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  disabled: boolean;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
}

export function InputArea({ value, onChange, onSend, disabled, textareaRef, onKeyDown }: Props) {
  const { t } = useTranslation();

  return (
    <form className="input-area" onSubmit={(e) => { e.preventDefault(); onSend(); }}>
      <textarea
        ref={textareaRef}
        rows={2}
        placeholder={t('chat.placeholder')}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        disabled={disabled}
      />
      <button type="submit" disabled={!value.trim() || disabled}>{t('chat.send')}</button>
    </form>
  );
}
