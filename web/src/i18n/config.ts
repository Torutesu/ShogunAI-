export const locales = ['en', 'ja', 'es', 'de'] as const;
export type Locale = (typeof locales)[number];
export const defaultLocale: Locale = 'en';
export const LOCALE_COOKIE = 'locale';

export const localeNames: Record<Locale, string> = {
  en: 'English',
  ja: '日本語',
  es: 'Español',
  de: 'Deutsch',
};

export function isLocale(v: unknown): v is Locale {
  return typeof v === 'string' && (locales as readonly string[]).includes(v);
}
