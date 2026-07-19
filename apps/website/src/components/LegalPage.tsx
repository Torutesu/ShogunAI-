import { PageHeader, PageShell } from '@/components/PageShell';
import type { Dictionary } from '@/i18n/dictionaries';

/** Shared frame for /privacy and /terms. Content is placeholder. */
export function LegalPage({ t, title }: { t: Dictionary; title: string }) {
  const lp = t.legalPage;
  return (
    <PageShell>
      <PageHeader eyebrow={t.footer.legal.title} title={title} sub={lp.updated} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x max-w-[720px]">
          <p className="text-[17px] leading-relaxed text-muted">{lp.intro}</p>
          <div className="mt-10 grid gap-8">
            {lp.sections.map((s) => (
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
