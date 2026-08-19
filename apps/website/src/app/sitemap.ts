import type { MetadataRoute } from 'next';
import { getAllPosts } from '@/lib/blog';
import { siteConfig } from '@/lib/site';
import { featurePages, useCasePages } from '@/lib/marketing-content';

export default function sitemap(): MetadataRoute.Sitemap {
  const base = siteConfig.url;
  const staticRoutes = [
    ['', '2026-08-12'],
    ['/blog', '2026-08-12'],
    ['/blog/category/ai-memory', '2026-08-12'],
    ['/blog/category/work-context', '2026-08-12'],
    ['/blog/category/comparisons', '2026-08-12'],
    ['/blog/category/privacy', '2026-08-12'],
    ['/blog/category/product', '2026-08-12'],
    ['/careers', '2026-07-19'],
    ['/about', '2026-07-19'],
    ['/privacy', '2026-07-19'],
    ['/terms', '2026-07-19'],
    ['/rules', '2026-07-19'],
    ['/features', '2026-08-15'],
    ['/use-cases', '2026-08-15'],
    ['/integrations', '2026-08-15'],
    ['/security', '2026-08-15'],
    ['/pricing', '2026-08-15'],
    ['/compare', '2026-08-15'],
    ...featurePages.map((page) => [`/features/${page.slug}`, '2026-08-15']),
    ...useCasePages.map((page) => [`/use-cases/${page.slug}`, '2026-08-15']),
  ].map(([path, lastModified]) => ({
    url: `${base}${path}`,
    lastModified: new Date(lastModified),
    changeFrequency: 'weekly' as const,
    priority: path === '' ? 1 : 0.7,
  }));

  const posts = siteConfig.locales.flatMap((locale) =>
    getAllPosts(locale).map((p) => ({
      url: `${base}/${locale}/blog/${p.slug}`,
      lastModified: new Date(p.date),
      changeFrequency: 'monthly' as const,
      priority: 0.55,
    })),
  );

  const localizedRoutes = ['', '/blog', '/blog/category/ai-memory', '/blog/category/work-context', '/blog/category/comparisons', '/blog/category/privacy', '/blog/category/product']
    .flatMap((path) => siteConfig.locales.map((locale) => ({
      url: `${base}/${locale}${path}`,
      lastModified: new Date('2026-08-13'),
      changeFrequency: 'weekly' as const,
      priority: path === '' ? 0.95 : 0.55,
    })));

  const localizedMarketingPaths = [
    '/features',
    '/use-cases',
    '/integrations',
    '/security',
    '/pricing',
    '/compare',
    ...featurePages.map((page) => `/features/${page.slug}`),
    ...useCasePages.map((page) => `/use-cases/${page.slug}`),
  ].flatMap((path) => siteConfig.locales.map((locale) => ({
    url: `${base}/${locale}${path}`,
    lastModified: new Date('2026-08-15'),
    changeFrequency: 'weekly' as const,
    priority: path === '/features' || path === '/use-cases' ? 0.75 : 0.65,
  })));

  return [...staticRoutes, ...localizedRoutes, ...localizedMarketingPaths, ...posts];
}
