import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import matter from 'gray-matter';

const BLOG_DIR = join(process.cwd(), 'content', 'blog');

export type PostMeta = {
  slug: string;
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

/** All posts, newest first. Reads content/blog/*.mdx at build time. */
export function getAllPosts(): PostMeta[] {
  let files: string[] = [];
  try {
    files = readdirSync(BLOG_DIR).filter((f) => f.endsWith('.mdx'));
  } catch {
    return []; // no content dir yet
  }
  const posts = files.map((file) => {
    const slug = file.replace(/\.mdx$/, '');
    const raw = readFileSync(join(BLOG_DIR, file), 'utf8');
    const { data, content } = matter(raw);
    return {
      slug,
      title: String(data.title ?? slug),
      description: String(data.description ?? ''),
      date: String(data.date ?? '1970-01-01'),
      category: String(data.category ?? 'Product'),
      author: String(data.author ?? 'ShogunAI'),
      readingMinutes: readingMinutes(content),
    } satisfies PostMeta;
  });
  return posts.sort((a, b) => (a.date < b.date ? 1 : -1));
}

export function getPost(slug: string): Post | null {
  try {
    const raw = readFileSync(join(BLOG_DIR, `${slug}.mdx`), 'utf8');
    const { data, content } = matter(raw);
    return {
      slug,
      title: String(data.title ?? slug),
      description: String(data.description ?? ''),
      date: String(data.date ?? '1970-01-01'),
      category: String(data.category ?? 'Product'),
      author: String(data.author ?? 'ShogunAI'),
      readingMinutes: readingMinutes(content),
      content,
    };
  } catch {
    return null;
  }
}
