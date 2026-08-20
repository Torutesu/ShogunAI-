import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { MarketingContentPage } from '@/components/MarketingContentPage';
import { getI18n } from '@/i18n/server';
import { isLocale, locales } from '@/i18n/config';
import { findMarketingPage, getFeaturePages } from '@/lib/marketing-content';
import { localizedAlternates, siteConfig } from '@/lib/site';

const sectionLabels = { en: 'Features', ja: '機能', es: 'Funciones', de: 'Funktionen' } as const;

export function generateStaticParams() {
  return locales.flatMap((locale) => getFeaturePages(locale).map(({ slug }) => ({ locale, slug })));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string; slug: string }> }): Promise<Metadata> {
  const { locale, slug } = await params;
  if (!isLocale(locale)) return {};
  const page = findMarketingPage(getFeaturePages(locale), slug);
  if (!page) return {};
  const url = `${siteConfig.url}/${locale}/features/${slug}`;
  return {
    title: page.title,
    description: page.description,
    alternates: { canonical: url, languages: localizedAlternates(`/features/${slug}`) },
    openGraph: { title: page.title, description: page.description, url },
    twitter: { card: 'summary_large_image', title: page.title, description: page.description },
  };
}

export default async function LocalizedFeaturePage({ params }: { params: Promise<{ locale: string; slug: string }> }) {
  const { locale, slug } = await params;
  if (!isLocale(locale)) notFound();
  const page = findMarketingPage(getFeaturePages(locale), slug);
  if (!page) notFound();
  const { t } = await getI18n(locale);
  return <MarketingContentPage page={page} section="features" sectionLabel={sectionLabels[locale]} t={t} locale={locale} />;
}
