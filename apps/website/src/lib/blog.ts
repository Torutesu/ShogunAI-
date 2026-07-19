import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import matter from 'gray-matter';

const BLOG_DIR = join(process.cwd(), 'content', 'blog');
const LOCALES = ['ja', 'es', 'de'] as const; // 'en' is the default (no suffix)

export type PostMeta = {
  slug: string;
  locale: string;
  title: string;
  description: string;
  date: string; // ISO
  category: string;
  author: string;
  readingMinutes: number;
};

export type Post = PostMeta & { content: string };

function readingMinutes(text: string): number {
  const words = text.trim().split(/\s+/).length;
  return Math.max(1, Math.round(words / 200));
}

/** Split "slug.ja" → { base: "slug", locale: "ja" }; plain → locale "en". */
function parseName(nameNoExt: string): { base: string; locale: string } {
  const dot = nameNoExt.lastIndexOf('.');
  if (dot > 0) {
    const maybe = nameNoExt.slice(dot + 1);
    if ((LOCALES as readonly string[]).includes(maybe)) {
      return { base: nameNoExt.slice(0, dot), locale: maybe };
    }
  }
  return { base: nameNoExt, locale: 'en' };
}

function metaFrom(base: string, locale: string, raw: string): PostMeta {
  const { data, content } = matter(raw);
  return {
    slug: base,
    locale,
    title: String(data.title ?? base),
    description: String(data.description ?? ''),
    date: String(data.date ?? '1970-01-01'),
    category: String(data.category ?? 'Product'),
    author: String(data.author ?? 'ShogunAI'),
    readingMinutes: readingMinutes(content),
  };
}

/**
 * All posts for `locale`, newest first. A post may exist as `slug.mdx` (en)
 * and localized `slug.<locale>.mdx`; we return the requested locale's variant
 * when present, otherwise fall back to the English/default file.
 */
export function getAllPosts(locale = 'en'): PostMeta[] {
  let files: string[] = [];
  try {
    files = readdirSync(BLOG_DIR).filter((f) => f.endsWith('.mdx'));
  } catch {
    return [];
  }
  // base slug → { locale → filename }
  const byBase = new Map<string, Record<string, string>>();
  for (const file of files) {
    const { base, locale: loc } = parseName(file.replace(/\.mdx$/, ''));
    if (!byBase.has(base)) byBase.set(base, {});
    byBase.get(base)![loc] = file;
  }

  const posts: PostMeta[] = [];
  for (const [base, variants] of byBase) {
    const file = variants[locale] ?? variants.en;
    if (!file) continue; // a locale-only post with no en fallback is hidden elsewhere
    const usedLocale = variants[locale] ? locale : 'en';
    posts.push(metaFrom(base, usedLocale, readFileSync(join(BLOG_DIR, file), 'utf8')));
  }
  return posts.sort((a, b) => (a.date < b.date ? 1 : -1));
}

export function getPost(slug: string, locale = 'en'): Post | null {
  const candidates =
    locale === 'en' ? [`${slug}.mdx`] : [`${slug}.${locale}.mdx`, `${slug}.mdx`];
  for (const file of candidates) {
    try {
      const raw = readFileSync(join(BLOG_DIR, file), 'utf8');
      const usedLocale = file.endsWith(`.${locale}.mdx`) ? locale : 'en';
      const { content } = matter(raw);
      return { ...metaFrom(slug, usedLocale, raw), content };
    } catch {
      /* try next candidate */
    }
  }
  return null;
}
