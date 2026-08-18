import type { Metadata } from 'next';
import { PageHeader, PageShell } from '@/components/PageShell';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'About',
  description: 'ShogunAI is the operating system for the AI-native individual.',
  alternates: { canonical: '/about' },
};

export default async function AboutPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { t } = await getI18n(localeOverride);
  // Only the /[locale]/about entry point pins a locale, so only it gets prefixed
  // nav links; the un-prefixed route keeps resolving the visitor's locale itself.
  return (
    <PageShell locale={localeOverride}>
      <PageHeader eyebrow={t.about.eyebrow} title={t.about.title} sub={t.about.sub} />

      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x max-w-[760px]">
          <h2 className="font-display text-2xl font-semibold">{t.about.missionTitle}</h2>
          <p className="mt-3 text-[17px] leading-relaxed text-muted">{t.about.missionBody}</p>

          <h2 className="mt-14 font-display text-2xl font-semibold">{t.about.valuesTitle}</h2>
          <div className="mt-6 grid gap-6 sm:grid-cols-3">
            {t.about.values.map((v) => (
              <Card key={v.title} className="lift h-full">
                <h3 className="font-display text-lg font-semibold">{v.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted">{v.body}</p>
              </Card>
            ))}
          </div>

          <h2 className="mt-14 font-display text-2xl font-semibold">{t.about.teamTitle}</h2>
          <p className="mt-3 text-[17px] leading-relaxed text-muted">{t.about.teamNote}</p>
        </div>
      </section>
    </PageShell>
  );
}
