import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import PrivacyPage from '@/app/privacy/page';
import { getDictionary } from '@/i18n/dictionaries';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const dynamic = 'force-dynamic';

export function generateStaticParams() { return locales.map((locale) => ({ locale })); }

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> {
  const { locale } = await params;
  if (!isLocale(locale)) return {};
  const t = getDictionary(locale);
  const descriptions = {
    en: 'How ShogunAI handles website, waitlist, and product data.',
    ja: 'ShogunAIのウェブサイト、ウェイトリスト、プロダクトデータの取り扱いについて説明します。',
    es: 'Cómo trata ShogunAI los datos del sitio, la lista de espera y el producto.',
    de: 'Wie ShogunAI Website-, Wartelisten- und Produktdaten verarbeitet.',
  } as const;
  const title = t.legalPage.privacyTitle;
  const description = descriptions[locale];
  const url = `${siteConfig.url}/${locale}/privacy`;
  return { title, description, alternates: { canonical: url, languages: localizedAlternates('/privacy') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } };
}

export default async function Page({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return <PrivacyPage />;
}
