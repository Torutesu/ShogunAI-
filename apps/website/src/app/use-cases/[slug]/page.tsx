import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { MarketingContentPage } from '@/components/MarketingContentPage';
import { getI18n } from '@/i18n/server';
import { findMarketingPage, getUseCasePages } from '@/lib/marketing-content';
import { localizedAlternates } from '@/lib/site';

export function generateStaticParams() { return getUseCasePages('en').map(({ slug }) => ({ slug })); }

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  const page = findMarketingPage(getUseCasePages('en'), slug);
  if (!page) return {};
  return { title: page.title, description: page.description, alternates: { canonical: `/en/use-cases/${slug}`, languages: localizedAlternates(`/use-cases/${slug}`) }, openGraph: { title: page.title, description: page.description, url: `/en/use-cases/${slug}` }, twitter: { card: 'summary_large_image', title: page.title, description: page.description } };
}

export default async function UseCaseDetailPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const page = findMarketingPage(getUseCasePages('en'), slug);
  if (!page) notFound();
  const { t } = await getI18n();
  return <MarketingContentPage page={page} section="use-cases" sectionLabel="Use cases" t={t} locale="en" />;
}
