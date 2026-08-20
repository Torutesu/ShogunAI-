'use client';

import { useState } from 'react';
import type { PostMeta } from '@/lib/blog';
import { CATEGORY_KEYS, Cat, Cover, PostCard, categoryLabel, formatDate } from '@/components/PostCard';

export function BlogFilter({
  posts,
  categories,
  locale,
  minRead,
  more,
  empty,
  filterLabel,
  hrefPrefix = '',
}: {
  posts: PostMeta[];
  categories: string[];
  locale: string;
  minRead: string;
  more: string;
  empty: string;
  filterLabel: string;
  hrefPrefix?: string;
}) {
  const [active, setActive] = useState(0); // 0 = All
  const shown = active === 0 ? posts : posts.filter((p) => p.category === CATEGORY_KEYS[active]);
  const [featured, ...rest] = shown;

  return (
    <>
      <div className="mb-10 flex flex-wrap gap-2.5" aria-label={filterLabel}>
        {categories.map((c, i) => (
          <button
            key={c}
            type="button"
            onClick={() => setActive(i)}
            className={`cursor-pointer rounded-full border px-4 py-2 text-sm font-medium transition-colors ${
              i === active
                ? 'border-ink bg-ink text-on-ink'
                : 'border-border text-muted hover:border-ink/30 hover:text-ink'
            }`}
          >
            {c}
          </button>
        ))}
      </div>

      {shown.length === 0 && <p className="text-muted">{empty}</p>}

      {featured && (
        <a href={`${hrefPrefix}/blog/${featured.slug}`} className="group mb-14 block">
          <article className="grid items-center gap-8 md:grid-cols-2">
            <Cover className="aspect-[16/10] rounded-2xl border border-border" image={featured.image} alt={featured.title} />
            <div>
              <Cat label={categoryLabel(featured.category, categories)} solid />
              <h2 className="mt-4 font-display text-[clamp(22px,5vw,34px)] font-semibold leading-tight tracking-tight text-balance transition-colors group-hover:text-accent-strong">
                {featured.title}
              </h2>
              <p className="mt-4 text-[15px] leading-relaxed text-muted sm:text-[17px]">{featured.description}</p>
              <p className="mt-5 text-sm text-muted">{formatDate(featured.date, locale)}</p>
            </div>
          </article>
        </a>
      )}

      <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
        {rest.map((p) => (
          <PostCard key={p.slug} p={p} categories={categories} locale={locale} minRead={minRead} more={more} hrefPrefix={hrefPrefix} />
        ))}
      </div>
    </>
  );
}
