import { BLOG_DATA } from './blog-data.generated';

export type PostMeta = {
  slug: string;
  locale: string;
  title: string;
  description: string;
  date: string; // ISO
  category: string;
  author: string;
  image: string;
  readingMinutes: number;
};

export type Post = PostMeta & { html: string };

/**
 * All posts for `locale`, newest first. The generator materializes a record
 * for every supported locale, so translated listings never mix in English.
 */
export function getAllPosts(locale = 'en'): PostMeta[] {
  const bySlug = new Map<string, Map<string, (typeof BLOG_DATA)[number]>>();
  for (const entry of BLOG_DATA) {
    if (!bySlug.has(entry.slug)) bySlug.set(entry.slug, new Map());
    bySlug.get(entry.slug)!.set(entry.locale, entry);
  }

  const posts: PostMeta[] = [];
  for (const variants of bySlug.values()) {
    const entry = variants.get(locale);
    if (!entry) continue;
    const { html: _html, ...meta } = entry;
    posts.push({ ...meta });
  }
  return posts.sort((a, b) => (a.date < b.date ? 1 : -1));
}

export function getPost(slug: string, locale = 'en'): Post | null {
  const entry = BLOG_DATA.find((candidate) => candidate.slug === slug && candidate.locale === locale);
  return entry ? { ...entry } : null;
}
