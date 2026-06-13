import { useTranslation } from 'react-i18next';
import i18n, { SUPPORTED_LANGS, persistLanguage, SupportedLang } from '../i18n';

export function LanguageSwitcher() {
  const { t } = useTranslation();

  const handleChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const lang = e.target.value as SupportedLang;
    i18n.changeLanguage(lang);
    persistLanguage(lang);
  };

  return (
    <select
      value={i18n.language}
      onChange={handleChange}
      style={{
        padding: '4px 8px',
        border: '1px solid #ddd',
        borderRadius: 6,
        fontSize: 12,
        background: '#fff',
        cursor: 'pointer',
      }}
    >
      {SUPPORTED_LANGS.map((l) => (
        <option key={l.code} value={l.code}>{l.label}</option>
      ))}
    </select>
  );
}
