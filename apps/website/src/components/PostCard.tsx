import Image from 'next/image';
import type { PostMeta } from '@/lib/blog';

// Canonical category keys (post.category is stored in English); display labels
// come from the dictionary and line up by index with these.
export const CATEGORY_KEYS = ['All', 'Ideas', 'Product'];

export function categoryLabel(category: string, labels: string[]) {
  const index = CATEGORY_KEYS.indexOf(category);
  return index >= 0 ? labels[index] : category;
}

export function formatDate(iso: string, locale: string) {
  try {
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'long', day: 'numeric' }).format(new Date(iso));
  } catch {
    return iso;
  }
}

export function Cover({ className, image, alt }: { className?: string; image: string; alt: string }) {
  return (
    <div className={`relative overflow-hidden bg-[linear-gradient(135deg,var(--color-sky-soft),var(--color-cloud))] ${className ?? ''}`}>
      <Image src={image} alt={alt} fill sizes="(max-width: 768px) 100vw, 50vw" className="object-cover opacity-90 transition-transform duration-500 group-hover:scale-[1.03]" />
      <div className="absolute inset-0 bg-gradient-to-t from-[rgba(9,18,45,0.42)] via-transparent to-[rgba(0,76,252,0.08)]" />
      <div className="absolute inset-0 bg-[radial-gradient(60%_60%_at_30%_20%,rgba(0,76,252,0.12),transparent)]" />
      <div className="absolute -right-8 -top-10 size-40 rounded-full border border-accent/15" />
      <div className="absolute -bottom-16 -left-10 size-48 rounded-full border border-accent/10" />
      <div className="absolute inset-x-0 bottom-5 h-px bg-accent/15" />
    </div>
  );
}

export function Cat({ label, solid }: { label: string; solid?: boolean }) {
  return (
    <span
      className={`inline-flex items-center rounded-full px-3 py-1 text-xs font-semibold ${
        solid ? 'bg-accent text-white' : 'bg-sky-soft text-accent-strong'
      }`}
    >
      {label}
    </span>
  );
}

/**
 * The blog listing card. Shared by the blog index and by any marketing page
 * that features an article, so a featured card never drifts from the listing.
 * The whole card is the link — clicking anywhere on it opens the post.
 */
export function PostCard({ p, categories, locale, minRead, more, hrefPrefix }: { p: PostMeta; categories: string[]; locale: string; minRead: string; more: string; hrefPrefix: string }) {
  return (
    <a href={`${hrefPrefix}/blog/${p.slug}`} className="group block h-full">
      <article className="lift flex h-full flex-col overflow-hidden rounded-2xl border border-border bg-surface">
        <Cover className="h-40" image={p.image} alt={p.title} />
        <div className="flex flex-1 flex-col p-5">
          <div className="mb-3 flex items-center gap-2">
            <Cat label={categoryLabel(p.category, categories)} />
            <span className="text-xs text-muted">
              {p.readingMinutes} {minRead}
            </span>
          </div>
          <h3 className="font-display text-base font-semibold leading-snug tracking-tight sm:text-lg">{p.title}</h3>
          <p className="mt-2 line-clamp-2 text-sm leading-relaxed text-muted">{p.description}</p>
          <div className="mt-auto flex items-center justify-between pt-5 text-xs text-muted">
            <span>{formatDate(p.date, locale)}</span>
            <span className="font-medium text-accent opacity-0 transition-opacity group-hover:opacity-100">{more}</span>
          </div>
        </div>
      </article>
    </a>
  );
}
