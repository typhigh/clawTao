/**
 * i18n initialization — must be imported before React renders.
 */
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import zhCN from './locales/zh-CN.json';
import en from './locales/en.json';
import ja from './locales/ja.json';
import ru from './locales/ru.json';
import fr from './locales/fr.json';
import ko from './locales/ko.json';

export const SUPPORTED_LANGS = [
  { code: 'zh-CN', label: '中文' },
  { code: 'en', label: 'English' },
  { code: 'ja', label: '日本語' },
  { code: 'ru', label: 'Русский' },
  { code: 'fr', label: 'Français' },
  { code: 'ko', label: '한국어' },
] as const;

export type SupportedLang = (typeof SUPPORTED_LANGS)[number]['code'];

const DEFAULT_LANG: SupportedLang = 'zh-CN';

function getSavedLang(): SupportedLang {
  try {
    const raw = localStorage.getItem('clawtao-language');
    if (raw && SUPPORTED_LANGS.some((l) => l.code === raw)) {
      return raw as SupportedLang;
    }
  } catch { /* localStorage unavailable */ }
  return DEFAULT_LANG;
}

i18n.use(initReactI18next).init({
  resources: {
    'zh-CN': zhCN,
    en,
    ja,
    ru,
    fr,
    ko,
  },
  lng: getSavedLang(),
  fallbackLng: DEFAULT_LANG,
  nsSeparator: '.',
  keySeparator: false,
  interpolation: {
    escapeValue: false, // React already escapes
  },
});

export function persistLanguage(lang: SupportedLang): void {
  try {
    localStorage.setItem('clawtao-language', lang);
  } catch { /* noop */ }
}

export default i18n;
