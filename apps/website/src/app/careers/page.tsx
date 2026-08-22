import type { Metadata } from 'next';
import { PageHeader, PageShell } from '@/components/PageShell';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Careers',
  description: 'Build the OS for the AI-native individual.',
  alternates: { canonical: '/careers' },
};

export default async function CareersPage() {
  const { t } = await getI18n();
  return (
    <PageShell>
      <PageHeader eyebrow={t.careers.eyebrow} title={t.careers.title} sub={t.careers.sub} />

      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x">
          <h2 className="mb-6 font-display text-2xl font-semibold">{t.careers.perksTitle}</h2>
          <div className="grid gap-6 md:grid-cols-3">
            {t.careers.perks.map((p) => (
              <Card key={p.title} className="lift h-full">
                <h3 className="font-display text-lg font-semibold">{p.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted">{p.body}</p>
              </Card>
            ))}
          </div>

          <h2 className="mb-6 mt-16 font-display text-2xl font-semibold">{t.careers.openRolesTitle}</h2>
          <Card className="flex flex-col items-start gap-4 sm:flex-row sm:items-center sm:justify-between">
            <p className="text-muted">{t.careers.noRoles}</p>
            <Button asChild variant="secondary">
              <a href="mailto:info@shogunaios.com">{t.careers.apply}</a>
            </Button>
          </Card>
        </div>
      </section>
    </PageShell>
  );
}
