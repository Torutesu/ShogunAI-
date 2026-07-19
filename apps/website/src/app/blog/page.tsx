import type { Metadata } from 'next';
import { Reveal } from '@/components/animations/Reveal';
import { PageHeader, PageShell } from '@/components/PageShell';
import { getAllPosts, type PostMeta } from '@/lib/blog';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Blog',
  description: 'Field notes from building an OS for the AI-native individual.',
  alternates: {
    canonical: '/blog',
    types: { 'application/rss+xml': '/rss.xml' },
  },
};

function formatDate(iso: string, locale: string) {
  try {
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'long', day: 'numeric' }).format(
      new Date(iso),
    );
  } catch {
    return iso;
  }
}

/** Category pill. `solid` for the featured card, tinted for grid cards. */
function Cat({ label, solid }: { label: string; solid?: boolean }) {
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

/** Placeholder cover — gradient stand-in for a real image. */
function Cover({ className }: { className?: string }) {
  return (
    <div
      className={`relative overflow-hidden bg-[linear-gradient(135deg,var(--color-sky-soft),var(--color-cloud))] ${className ?? ''}`}
    >
      <div className="absolute inset-0 bg-[radial-gradient(60%_60%_at_30%_20%,rgba(0,166,244,0.12),transparent)]" />
    </div>
  );
}

function PostCard({
  p,
  locale,
  minRead,
  more,
}: {
  p: PostMeta;
  locale: string;
  minRead: string;
  more: string;
}) {
  return (
    <a href={`/blog/${p.slug}`} className="group block h-full">
      <article className="lift flex h-full flex-col overflow-hidden rounded-2xl border border-border bg-surface">
        <Cover className="h-40" />
        <div className="flex flex-1 flex-col p-5">
          <div className="mb-3 flex items-center gap-2">
            <Cat label={p.category} />
            <span className="text-xs text-muted">
              {p.readingMinutes} {minRead}
            </span>
          </div>
          <h3 className="font-display text-lg font-semibold leading-snug tracking-tight">{p.title}</h3>
          <p className="mt-2 line-clamp-2 text-sm leading-relaxed text-muted">{p.description}</p>
          <div className="mt-auto flex items-center justify-between pt-5 text-xs text-muted">
            <span>{formatDate(p.date, locale)}</span>
            <span className="font-medium text-accent opacity-0 transition-opacity group-hover:opacity-100">
              {more}
            </span>
          </div>
        </div>
      </article>
    </a>
  );
}

export default async function BlogIndex() {
  const { locale, t } = await getI18n();
  const posts = getAllPosts();
  const [featured, ...rest] = posts;

  return (
    <PageShell>
      <PageHeader eyebrow={t.blog.eyebrow} title={t.blog.title} sub={t.blog.sub} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x">
          {/* Filters — frame only */}
          <div className="mb-10 flex flex-wrap gap-2.5">
            {t.blog.categories.map((c, i) => (
              <span
                key={c}
                className={`cursor-pointer rounded-full border px-4 py-2 text-sm font-medium transition-colors ${
                  i === 0
                    ? 'border-ink bg-ink text-on-ink'
                    : 'border-border text-muted hover:border-ink/30 hover:text-ink'
                }`}
              >
                {c}
              </span>
            ))}
          </div>

          {/* Featured */}
          {featured && (
            <Reveal>
              <a href={`/blog/${featured.slug}`} className="group mb-14 block">
                <article className="grid items-center gap-8 md:grid-cols-2">
                  <Cover className="aspect-[16/10] rounded-2xl border border-border" />
                  <div>
                    <Cat label={featured.category} solid />
                    <h2 className="mt-4 font-display text-[clamp(24px,3vw,34px)] font-semibold leading-tight tracking-tight text-balance transition-colors group-hover:text-accent-strong">
                      {featured.title}
                    </h2>
                    <p className="mt-4 text-[17px] leading-relaxed text-muted">{featured.description}</p>
                    <p className="mt-5 text-sm text-muted">{formatDate(featured.date, locale)}</p>
                  </div>
                </article>
              </a>
            </Reveal>
          )}

          {/* Grid */}
          <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {rest.map((p, i) => (
              <Reveal key={p.slug} delay={(i % 3) * 0.06}>
                <PostCard p={p} locale={locale} minRead={t.blog.minRead} more={t.blog.readMore} />
              </Reveal>
            ))}
          </div>
        </div>
      </section>
    </PageShell>
  );
}
