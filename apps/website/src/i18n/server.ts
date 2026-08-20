import { cookies } from 'next/headers';
import { headers } from 'next/headers';
import { LOCALE_COOKIE, type Locale, defaultLocale, isLocale } from './config';
import { type Dictionary, getDictionary } from './dictionaries';

/** Read the active locale from the cookie (server components). */
export async function getLocale(): Promise<Locale> {
  const requestHeaders = await headers();
  const pathLocale = requestHeaders.get('x-shogun-locale');
  if (isLocale(pathLocale)) return pathLocale;
  const store = await cookies();
  const v = store.get(LOCALE_COOKIE)?.value;
  return isLocale(v) ? v : defaultLocale;
}

/** Resolve the active locale + its dictionary in one call. */
export async function getI18n(localeOverride?: Locale): Promise<{ locale: Locale; t: Dictionary }> {
  const locale = localeOverride ?? (await getLocale());
  return { locale, t: getDictionary(locale) };
}
