import type { Metadata } from 'next';
import { Reveal } from '@/components/animations/Reveal';
import { PageHeader, PageShell } from '@/components/PageShell';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import { getAllPosts } from '@/lib/blog';
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
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric' }).format(
      new Date(iso),
    );
  } catch {
    return iso;
  }
}

export default async function BlogIndex() {
  const { locale, t } = await getI18n();
  const posts = getAllPosts();

  return (
    <PageShell>
      <PageHeader eyebrow={t.blog.eyebrow} title={t.blog.title} sub={t.blog.sub} />
      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x">
          {/* Category filters — frame only */}
          <div className="mb-8 flex flex-wrap gap-2">
            {t.blog.categories.map((c, i) => (
              <span
                key={c}
                className={`rounded-full border px-3.5 py-1.5 text-sm font-medium ${
                  i === 0 ? 'border-ink bg-ink text-cloud' : 'border-border text-muted'
                }`}
              >
                {c}
              </span>
            ))}
          </div>

          <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {posts.map((p, i) => (
              <Reveal key={p.slug} delay={(i % 3) * 0.06}>
                <a href={`/blog/${p.slug}`} className="group block h-full">
                  <Card className="lift flex h-full flex-col">
                  <div className="mb-3 flex items-center gap-2">
                    <Badge>{p.category}</Badge>
                    <span className="text-xs text-muted">
                      {p.readingMinutes} {t.blog.minRead}
                    </span>
                  </div>
                  <h2 className="font-display text-xl font-semibold leading-snug tracking-tight">{p.title}</h2>
                  <p className="mt-2 line-clamp-3 text-sm leading-relaxed text-muted">{p.description}</p>
                  <div className="mt-auto flex items-center justify-between pt-5 text-xs text-muted">
                    <span>{formatDate(p.date, locale)}</span>
                    <span className="font-medium text-accent">{t.blog.readMore}</span>
                  </div>
                  </Card>
                </a>
              </Reveal>
            ))}
          </div>
        </div>
      </section>
    </PageShell>
  );
}
