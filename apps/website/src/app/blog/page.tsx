import type { Metadata } from 'next';
import { BlogFilter } from '@/components/BlogFilter';
import { PageHeader, PageShell } from '@/components/PageShell';
import { getAllPosts } from '@/lib/blog';
import { getI18n } from '@/i18n/server';
import { JsonLd } from '@/components/seo/JsonLd';
import { siteConfig } from '@/lib/site';
import { isLocale } from '@/i18n/config';
import { BlogInsights } from '@/components/BlogInsights';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Blog',
  description:
    'Practical guides to AI memory, work context, privacy, and connecting the tools knowledge workers already use.',
  alternates: {
    canonical: '/blog',
    languages: {
      en: '/en/blog',
      ja: '/ja/blog',
      es: '/es/blog',
      de: '/de/blog',
      'x-default': '/en/blog',
    },
    types: { 'application/rss+xml': '/rss.xml' },
  },
};

async function BlogIndex({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const { locale, t } = await getI18n(isLocale(requested) ? requested : undefined);
  const posts = getAllPosts(locale);
  const prefix = `/${locale}`;
  const itemListSchema = {
    '@context': 'https://schema.org',
    '@type': 'ItemList',
    name: 'ShogunAI Blog',
    itemListElement: posts.map((post, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      url: `${siteConfig.url}/blog/${post.slug}`,
      name: post.title,
    })),
  };

  return (
    <PageShell locale={locale}>
      <JsonLd data={itemListSchema} />
      <PageHeader eyebrow={t.blog.eyebrow} title={t.blog.title} sub={t.blog.sub} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x">
          <div className="grid gap-10 lg:grid-cols-[250px_minmax(0,1fr)] lg:items-start lg:gap-14">
            <BlogInsights locale={locale} />
            <BlogFilter
              posts={posts}
              categories={t.blog.categories}
              locale={locale}
              minRead={t.blog.minRead}
              more={t.blog.readMore}
              empty={t.blog.empty}
              filterLabel={t.blog.filterLabel}
              hrefPrefix={prefix}
            />
          </div>
        </div>
      </section>
    </PageShell>
  );
}

export default BlogIndex;
