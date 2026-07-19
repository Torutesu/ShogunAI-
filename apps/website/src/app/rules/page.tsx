import type { Metadata } from 'next';
import { PageHeader, PageShell } from '@/components/PageShell';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Early-access rewards — official rules',
  description: 'Official rules for the ShogunAI early-access rewards program. No purchase necessary.',
  alternates: { canonical: '/rules' },
};

export default async function RulesPage() {
  const { t } = await getI18n();
  const r = t.rules;
  return (
    <PageShell>
      <PageHeader eyebrow={t.footer.legal.title} title={r.title} sub={r.updated} />
      <section className="py-[clamp(40px,6vw,72px)]">
        <div className="container-x max-w-[720px]">
          <p className="text-[17px] leading-relaxed text-muted">{r.intro}</p>
          <div className="mt-10 grid gap-8">
            {r.sections.map((s) => (
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
