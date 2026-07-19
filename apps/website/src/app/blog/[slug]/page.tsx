import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { MDXRemote } from 'next-mdx-remote/rsc';
import { PageShell } from '@/components/PageShell';
import { Badge } from '@/components/ui/badge';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { getAllPosts, getPost } from '@/lib/blog';
import { siteConfig } from '@/lib/site';
import { getI18n } from '@/i18n/server';

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
    alternates: { canonical: `/blog/${slug}` },
    openGraph: { type: 'article', title: post.title, description: post.description },
  };
}

export default async function BlogPost({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const { t, locale } = await getI18n();
  const post = getPost(slug, locale);
  if (!post) notFound();

  const articleSchema = {
    '@context': 'https://schema.org',
    '@type': 'Article',
    headline: post.title,
    description: post.description,
    datePublished: post.date,
    author: { '@type': 'Organization', name: post.author },
    publisher: { '@type': 'Organization', name: siteConfig.name },
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
    <PageShell>
      <JsonLd data={articleSchema} />
      <JsonLd
        data={breadcrumbSchema([
          { name: 'Blog', url: `${siteConfig.url}/blog` },
          { name: post.title, url: `${siteConfig.url}/blog/${slug}` },
        ])}
      />
      <article className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x max-w-[720px]">
          <a href="/blog" className="text-sm font-medium text-accent hover:text-accent-strong">
            {t.blog.backToBlog}
          </a>
          <div className="mt-6 flex items-center gap-3">
            <Badge>{post.category}</Badge>
            <span className="text-xs text-muted">
              {dateLabel} · {post.readingMinutes} {t.blog.minRead}
            </span>
          </div>
          <h1 className="mt-4 font-display text-[clamp(30px,4.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.02em] text-balance">
            {post.title}
          </h1>
          <p className="mt-4 text-[17px] leading-relaxed text-muted">{post.description}</p>

          <div
            className="mt-10 text-[17px] leading-[1.75] text-ink [&_blockquote]:my-6 [&_blockquote]:border-l-2 [&_blockquote]:border-accent [&_blockquote]:pl-4 [&_blockquote]:text-muted [&_code]:rounded [&_code]:bg-cloud [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.9em] [&_h2]:mt-10 [&_h2]:font-display [&_h2]:text-2xl [&_h2]:font-semibold [&_h3]:mt-8 [&_h3]:font-display [&_h3]:text-xl [&_h3]:font-semibold [&_li]:my-1 [&_p]:mt-5 [&_pre]:mt-5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-border [&_pre]:bg-cloud [&_pre]:p-4 [&_ul]:mt-5 [&_ul]:list-disc [&_ul]:pl-5"
          >
            <MDXRemote source={post.content} />
          </div>
        </div>
      </article>
    </PageShell>
  );
}
