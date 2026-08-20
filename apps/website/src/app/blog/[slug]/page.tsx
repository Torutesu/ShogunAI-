import type { Metadata } from 'next';
import Image from 'next/image';
import { notFound } from 'next/navigation';
import { PageShell } from '@/components/PageShell';
import { Badge } from '@/components/ui/badge';
import { JsonLd, breadcrumbSchema, publisherSchema } from '@/components/seo/JsonLd';
import { getAllPosts, getPost } from '@/lib/blog';
import { siteConfig } from '@/lib/site';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';

export const dynamic = 'force-dynamic';

export function generateStaticParams() {
  return getAllPosts().map((p) => ({ slug: p.slug }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const { locale } = await getI18n();
  const post = getPost(slug, locale);
  if (!post) return {};
  return {
    title: post.title,
    description: post.description,
    alternates: {
      canonical: `/blog/${slug}`,
      languages: {
        en: `${siteConfig.url}/en/blog/${slug}`,
        ja: `${siteConfig.url}/ja/blog/${slug}`,
        es: `${siteConfig.url}/es/blog/${slug}`,
        de: `${siteConfig.url}/de/blog/${slug}`,
        'x-default': `${siteConfig.url}/en/blog/${slug}`,
      },
    },
    openGraph: {
      type: 'article',
      title: post.title,
      description: post.description,
      url: `${siteConfig.url}/blog/${slug}`,
      publishedTime: post.date,
      modifiedTime: post.date,
      section: post.category,
      images: [{ url: '/og-image.png', width: 1200, height: 630, alt: post.title }],
    },
    twitter: {
      card: 'summary_large_image',
      title: post.title,
      description: post.description,
      images: ['/og-image.png'],
    },
  };
}

async function BlogPost({ params, searchParams }: { params: Promise<{ slug: string }>; searchParams: Promise<{ _locale?: string }> }) {
  const { slug } = await params;
  const requested = (await searchParams)._locale;
  const { t, locale } = await getI18n(isLocale(requested) ? requested : undefined);
  const post = getPost(slug, locale);
  if (!post) notFound();
  const relatedPosts = getAllPosts(locale).filter((candidate) => candidate.slug !== post.slug).slice(0, 3);
  const hrefPrefix = `/${locale}`;
  const categoryLabel = (category: string) => {
    const keys = ['Ideas', 'Product'];
    const index = keys.indexOf(category);
    return index >= 0 ? t.blog.categories[index + 1] : category;
  };

  const articleSchema = {
    '@context': 'https://schema.org',
    '@type': 'Article',
    headline: post.title,
    description: post.description,
    image: `${siteConfig.url}${post.image}`,
    datePublished: post.date,
    dateModified: post.date,
    articleSection: post.category,
    mainEntityOfPage: `${siteConfig.url}${hrefPrefix}/blog/${slug}`,
    author: { '@type': 'Organization', name: post.author, url: siteConfig.url },
    publisher: publisherSchema,
  };

  const dateLabel = (() => {
    try {
      return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'long', day: 'numeric' }).format(
        new Date(post.date),
      );
    } catch {
      return post.date;
    }
  })();

  return (
    <PageShell locale={locale}>
      <JsonLd data={articleSchema} />
      <JsonLd
        data={breadcrumbSchema([
          { name: t.blog.eyebrow, url: `${siteConfig.url}${hrefPrefix}/blog` },
          { name: post.title, url: `${siteConfig.url}${hrefPrefix}/blog/${slug}` },
        ])}
      />
      <article className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x max-w-[720px]">
          <nav aria-label="Breadcrumb" className="text-sm text-muted">
          <a href={`${hrefPrefix}/blog`} className="font-medium text-accent hover:text-accent-strong">
            {t.blog.backToBlog}
            </a>
            <span className="mx-2" aria-hidden="true">/</span>
            <span>{categoryLabel(post.category)}</span>
          </nav>
          <div className="mt-6 flex items-center gap-3">
            <Badge>{categoryLabel(post.category)}</Badge>
            <span className="text-xs text-muted">
              {dateLabel} · {post.readingMinutes} {t.blog.minRead}
            </span>
          </div>
          <h1 className="mt-4 font-display text-[clamp(30px,4.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.02em] text-balance">
            {post.title}
          </h1>
          <p className="mt-4 text-[17px] leading-relaxed text-muted">{post.description}</p>

          <div className="relative mt-8 aspect-[16/9] overflow-hidden rounded-2xl border border-border bg-cloud">
            <Image src={post.image} alt={post.title} fill sizes="(max-width: 720px) 100vw, 720px" className="object-cover" priority />
          </div>

          <div
            className="mt-10 text-[17px] leading-[1.75] text-ink [&_blockquote]:my-6 [&_blockquote]:border-l-2 [&_blockquote]:border-accent [&_blockquote]:pl-4 [&_blockquote]:text-muted [&_code]:rounded [&_code]:bg-cloud [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.9em] [&_h2]:mt-10 [&_h2]:font-display [&_h2]:text-2xl [&_h2]:font-semibold [&_h3]:mt-8 [&_h3]:font-display [&_h3]:text-xl [&_h3]:font-semibold [&_img]:mx-auto [&_img]:my-8 [&_img]:w-full [&_img]:max-w-[420px] [&_img]:rounded-xl [&_img]:border [&_img]:border-border [&_li]:my-1 [&_p]:mt-5 [&_pre]:mt-5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-border [&_pre]:bg-cloud [&_pre]:p-4 [&_ul]:mt-5 [&_ul]:list-disc [&_ul]:pl-5"
            dangerouslySetInnerHTML={{ __html: post.html }}
          />

          {relatedPosts.length > 0 && (
            <aside className="mt-16 border-t border-border pt-8" aria-label={t.blog.relatedLabel}>
              <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.blog.keepReading}</p>
              <div className="mt-4 grid gap-3 sm:grid-cols-3">
                {relatedPosts.map((related) => (
                  <a
                    key={related.slug}
                    href={`${hrefPrefix}/blog/${related.slug}`}
                    className="group rounded-xl border border-border bg-cloud/40 p-4 transition-colors hover:border-accent/40 hover:bg-sky-soft/50"
                  >
                    <span className="text-[11px] font-semibold uppercase tracking-wide text-accent-strong">{categoryLabel(related.category)}</span>
                    <h2 className="mt-2 text-sm font-semibold leading-snug group-hover:text-accent-strong">{related.title}</h2>
                  </a>
                ))}
              </div>
            </aside>
          )}
        </div>
      </article>
    </PageShell>
  );
}

export default BlogPost;
