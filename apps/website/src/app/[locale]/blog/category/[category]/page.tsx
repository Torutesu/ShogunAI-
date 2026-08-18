import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import BlogCategory from '@/app/blog/category/[category]/page';
import { isLocale, locales } from '@/i18n/config';
import { BLOG_CATEGORY_SLUGS, getBlogCategoryCopy, isBlogCategorySlug } from '@/lib/blog-categories';
import { siteConfig } from '@/lib/site';

export function generateStaticParams() {
  return locales.flatMap((locale) => BLOG_CATEGORY_SLUGS.map((category) => ({ locale, category })));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string; category: string }> }): Promise<Metadata> {
  const { locale, category } = await params;
  if (!isLocale(locale) || !isBlogCategorySlug(category)) return {};
  const copy = getBlogCategoryCopy(category, locale);
  return {
    title: copy.title,
    description: copy.description,
    alternates: {
      canonical: `/${locale}/blog/category/${category}`,
      languages: {
        en: `${siteConfig.url}/en/blog/category/${category}`,
        ja: `${siteConfig.url}/ja/blog/category/${category}`,
        es: `${siteConfig.url}/es/blog/category/${category}`,
        de: `${siteConfig.url}/de/blog/category/${category}`,
        'x-default': `${siteConfig.url}/en/blog/category/${category}`,
      },
    },
    openGraph: { type: 'website', title: copy.title, description: copy.description },
  };
}

export default async function LocalizedBlogCategory({ params }: { params: Promise<{ locale: string; category: string }> }) {
  const { locale, category } = await params;
  if (!isLocale(locale) || !isBlogCategorySlug(category)) notFound();
  return <BlogCategory params={Promise.resolve({ category })} searchParams={Promise.resolve({ _locale: locale })} />;
}
