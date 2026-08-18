import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { MarketingContentPage } from '@/components/MarketingContentPage';
import { getI18n } from '@/i18n/server';
import { isLocale, locales } from '@/i18n/config';
import { findMarketingPage, getUseCasePages } from '@/lib/marketing-content';
import { localizedAlternates, siteConfig } from '@/lib/site';

const sectionLabels = { en: 'Use cases', ja: '活用事例', es: 'Casos de uso', de: 'Anwendungsfälle' } as const;

export function generateStaticParams() {
  return locales.flatMap((locale) => getUseCasePages(locale).map(({ slug }) => ({ locale, slug })));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string; slug: string }> }): Promise<Metadata> {
  const { locale, slug } = await params;
  if (!isLocale(locale)) return {};
  const page = findMarketingPage(getUseCasePages(locale), slug);
  if (!page) return {};
  const url = `${siteConfig.url}/${locale}/use-cases/${slug}`;
  return {
    title: page.title,
    description: page.description,
    alternates: { canonical: url, languages: localizedAlternates(`/use-cases/${slug}`) },
    openGraph: { title: page.title, description: page.description, url },
    twitter: { card: 'summary_large_image', title: page.title, description: page.description },
  };
}

export default async function LocalizedUseCasePage({ params }: { params: Promise<{ locale: string; slug: string }> }) {
  const { locale, slug } = await params;
  if (!isLocale(locale)) notFound();
  const page = findMarketingPage(getUseCasePages(locale), slug);
  if (!page) notFound();
  const { t } = await getI18n(locale);
  return <MarketingContentPage page={page} section="use-cases" sectionLabel={sectionLabels[locale]} t={t} locale={locale} />;
}
