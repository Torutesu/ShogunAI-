import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { BlogFilter } from '@/components/BlogFilter';
import { JsonLd } from '@/components/seo/JsonLd';
import { PageHeader, PageShell } from '@/components/PageShell';
import { getAllPosts } from '@/lib/blog';
import { BLOG_CATEGORY_SLUGS, BLOG_TOPIC_LABEL, getBlogCategoryCopy, isBlogCategorySlug } from '@/lib/blog-categories';
import { getI18n } from '@/i18n/server';
import { siteConfig } from '@/lib/site';
import { isLocale } from '@/i18n/config';

export function generateStaticParams() {
  return BLOG_CATEGORY_SLUGS.map((category) => ({ category }));
}

export async function generateMetadata({ params }: { params: Promise<{ category: string }> }): Promise<Metadata> {
  const { category } = await params;
  if (!isBlogCategorySlug(category)) return {};
  const content = getBlogCategoryCopy(category, 'en');
  return {
    title: content.title,
    description: content.description,
    alternates: {
      canonical: `/blog/category/${category}`,
      languages: {
        en: `${siteConfig.url}/en/blog/category/${category}`,
        ja: `${siteConfig.url}/ja/blog/category/${category}`,
        es: `${siteConfig.url}/es/blog/category/${category}`,
        de: `${siteConfig.url}/de/blog/category/${category}`,
        'x-default': `${siteConfig.url}/en/blog/category/${category}`,
      },
    },
    openGraph: { type: 'website', title: content.title, description: content.description },
  };
}

async function BlogCategory({ params, searchParams }: { params: Promise<{ category: string }>; searchParams: Promise<{ _locale?: string }> }) {
  const { category } = await params;
  if (!isBlogCategorySlug(category)) notFound();
  const requested = (await searchParams)._locale;
  const { locale, t } = await getI18n(isLocale(requested) ? requested : undefined);
  const content = getBlogCategoryCopy(category, locale);
  const prefix = `/${locale}`;
  const posts = getAllPosts(locale).filter((post) => post.category === content.key);
  const itemListSchema = {
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: content.title,
    description: content.description,
    url: `${siteConfig.url}${prefix}/blog/category/${category}`,
    mainEntity: {
      '@type': 'ItemList',
      itemListElement: posts.map((post, index) => ({
        '@type': 'ListItem',
        position: index + 1,
        url: `${siteConfig.url}${prefix}/blog/${post.slug}`,
        name: post.title,
      })),
    },
  };

  return (
    <PageShell locale={locale}>
      <JsonLd data={itemListSchema} />
      <PageHeader eyebrow={BLOG_TOPIC_LABEL[locale]} title={content.title} sub={content.description} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x">
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
      </section>
    </PageShell>
  );
}

export default BlogCategory;
