import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import TermsPage from '@/app/terms/page';
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
    en: 'Terms governing the ShogunAI website, waitlist, and related services.',
    ja: 'ShogunAIのウェブサイト、ウェイトリスト、関連サービスに適用される利用規約です。',
    es: 'Términos aplicables al sitio, la lista de espera y los servicios relacionados de ShogunAI.',
    de: 'Bedingungen für die ShogunAI-Website, Warteliste und verbundene Dienste.',
  } as const;
  const title = t.legalPage.termsTitle;
  const description = descriptions[locale];
  const url = `${siteConfig.url}/${locale}/terms`;
  return { title, description, alternates: { canonical: url, languages: localizedAlternates('/terms') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } };
}

export default async function Page({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return <TermsPage />;
}
