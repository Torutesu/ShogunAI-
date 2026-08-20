import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import BlogPost from '@/app/blog/[slug]/page';
import { isLocale, locales } from '@/i18n/config';
import { getAllPosts, getPost } from '@/lib/blog';
import { siteConfig } from '@/lib/site';

export function generateStaticParams() {
  return locales.flatMap((locale) => getAllPosts(locale).map((post) => ({ locale, slug: post.slug })));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string; slug: string }> }): Promise<Metadata> {
  const { locale, slug } = await params;
  if (!isLocale(locale)) return {};
  const post = getPost(slug, locale);
  if (!post) return {};
  return {
    title: post.title,
    description: post.description,
    alternates: {
      canonical: `/${locale}/blog/${slug}`,
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
      url: `${siteConfig.url}/${locale}/blog/${slug}`,
      images: [{ url: post.image, width: 1536, height: 1024, alt: post.title }],
    },
  };
}

export default async function LocalizedBlogPost({ params }: { params: Promise<{ locale: string; slug: string }> }) {
  const { locale, slug } = await params;
  if (!isLocale(locale) || !getPost(slug, locale)) notFound();
  return <BlogPost params={Promise.resolve({ slug })} searchParams={Promise.resolve({ _locale: locale })} />;
}
