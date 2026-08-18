import { PageHeader, PageShell } from '@/components/PageShell';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

export function LegalPage({
  t,
  locale,
  title,
  updated,
  intro,
  sections,
}: {
  t: Dictionary;
  locale?: Locale;
  title: string;
  updated?: string;
  intro?: string;
  sections?: { h: string; p: string }[];
}) {
  const lp = t.legalPage;
  const resolvedUpdated = updated ?? lp.updated;
  const resolvedIntro = intro ?? lp.intro;
  const resolvedSections = sections ?? lp.sections;

  return (
    <PageShell locale={locale}>
      <PageHeader eyebrow={t.footer.legal.title} title={title} sub={resolvedUpdated} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x max-w-[720px]">
          <p className="text-[17px] leading-relaxed text-muted">{resolvedIntro}</p>
          <div className="mt-10 grid gap-8">
            {resolvedSections.map((s) => (
              <div key={s.h}>
                <h2 className="font-display text-xl font-semibold">{s.h}</h2>
                <p className="mt-2 text-[15px] leading-relaxed text-muted">{s.p}</p>
              </div>
            ))}
          </div>
        </div>
      </section>
    </PageShell>
  );
}
