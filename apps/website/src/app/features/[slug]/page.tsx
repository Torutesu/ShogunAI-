import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { MarketingContentPage } from '@/components/MarketingContentPage';
import { getDictionary } from '@/i18n/dictionaries';
import { findMarketingPage, getFeaturePages } from '@/lib/marketing-content';
import { localizedAlternates } from '@/lib/site';

export function generateStaticParams() { return getFeaturePages('en').map(({ slug }) => ({ slug })); }

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  const page = findMarketingPage(getFeaturePages('en'), slug);
  if (!page) return {};
  return { title: page.title, description: page.description, alternates: { canonical: `/en/features/${slug}`, languages: localizedAlternates(`/features/${slug}`) }, openGraph: { title: page.title, description: page.description, url: `/en/features/${slug}` }, twitter: { card: 'summary_large_image', title: page.title, description: page.description } };
}

export default async function FeatureDetailPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const page = findMarketingPage(getFeaturePages('en'), slug);
  if (!page) notFound();
  // This route is the un-prefixed English variant; the localized copy lives under
  // /[locale]/features/[slug]. Pin the dictionary to `en` so the shell, the body,
  // and the CTA never disagree because of the visitor's locale cookie.
  const t = getDictionary('en');
  return <MarketingContentPage page={page} section="features" sectionLabel="Features" t={t} locale="en" />;
}
