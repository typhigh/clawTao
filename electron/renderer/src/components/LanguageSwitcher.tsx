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
    <span className="settings-inline-select-wrap">
      <select
        className="settings-inline-select"
        value={i18n.language}
        onChange={handleChange}
      >
        {SUPPORTED_LANGS.map((l) => (
          <option key={l.code} value={l.code}>{l.label}</option>
        ))}
      </select>
    </span>
  );
}
