import { notFound, permanentRedirect } from 'next/navigation';
import { isLocale } from '@/i18n/config';

const comparisonSlugs = new Set(['shogunai-vs-notion', 'shogunai-vs-mem', 'shogunai-vs-glean']);

export default async function LocalizedComparisonRedirect({ params }: { params: Promise<{ locale: string; slug: string }> }) {
  const { locale, slug } = await params;
  if (!isLocale(locale) || !comparisonSlugs.has(slug)) notFound();
  permanentRedirect(`/${locale}/blog/${slug}`);
}
