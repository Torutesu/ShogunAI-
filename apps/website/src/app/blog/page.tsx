import type { Metadata } from 'next';
import { BlogFilter } from '@/components/BlogFilter';
import { PageHeader, PageShell } from '@/components/PageShell';
import { getAllPosts } from '@/lib/blog';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Blog',
  description: 'Field notes from building an OS for the AI-native individual.',
  alternates: {
    canonical: '/blog',
    types: { 'application/rss+xml': '/rss.xml' },
  },
};

export default async function BlogIndex() {
  const { locale, t } = await getI18n();
  const posts = getAllPosts(locale);

  return (
    <PageShell>
      <PageHeader eyebrow={t.blog.eyebrow} title={t.blog.title} sub={t.blog.sub} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x">
          <BlogFilter
            posts={posts}
            categories={t.blog.categories}
            locale={locale}
            minRead={t.blog.minRead}
            more={t.blog.readMore}
          />
        </div>
      </section>
    </PageShell>
  );
}
