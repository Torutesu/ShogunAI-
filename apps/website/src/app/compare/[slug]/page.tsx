import { notFound, permanentRedirect } from 'next/navigation';

const comparisonSlugs = new Set(['shogunai-vs-notion', 'shogunai-vs-mem', 'shogunai-vs-glean']);

export default async function ComparisonRedirect({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  if (!comparisonSlugs.has(slug)) notFound();
  permanentRedirect(`/blog/${slug}`);
}
