import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { siteConfig } from '@/lib/site';
import { localizedAlternates } from '@/lib/site';
import { getUseCasePages } from '@/lib/marketing-content';

export const metadata: Metadata = {
  title: 'Use cases — AI memory for knowledge work',
  description: 'See how founders, product and engineering teams, and consultants use private AI memory to recall context and complete work.',
  alternates: { canonical: '/en/use-cases', languages: localizedAlternates('/use-cases') },
};

const copy = {
  en: { eyebrow: 'Use cases', title: 'Built for people who move between contexts all day', sub: 'ShogunAI adapts to the work around you—keeping the right history available without forcing every role into the same workflow.', more: 'Explore workflow' },
  ja: { eyebrow: '活用事例', title: '一日の中で、いくつもの文脈を行き来する人のために', sub: 'すべての職種を同じワークフローに押し込めることなく、その人の仕事に必要な履歴をShogunAIがすぐに取り出せる状態にします。', more: '活用方法を見る' },
  es: { eyebrow: 'Casos de uso', title: 'Para quienes cambian de contexto durante todo el día', sub: 'ShogunAI se adapta a tu trabajo y mantiene disponible el historial adecuado sin imponer el mismo flujo a todos los roles.', more: 'Explorar flujo' },
  de: { eyebrow: 'Anwendungsfälle', title: 'Für Menschen, die den ganzen Tag zwischen Kontexten wechseln', sub: 'ShogunAI passt sich deiner Arbeit an und hält den richtigen Verlauf verfügbar, ohne jede Rolle in denselben Workflow zu zwingen.', more: 'Workflow ansehen' },
} as const;

export default async function UseCasesPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const pages = getUseCasePages(locale);
  const c = copy[locale];
  const prefix = `/${locale}`;
  return (
    <PageShell locale={locale}>
      <JsonLd data={breadcrumbSchema([{ name: 'Home', url: `${siteConfig.url}${prefix}` }, { name: c.eyebrow, url: `${siteConfig.url}${prefix}/use-cases` }])} />
      <PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.sub} />
      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-6 lg:grid-cols-3">
          {pages.map((page) => (
            <Card key={page.slug} className="lift flex h-full flex-col rounded-[26px] p-7">
              <span className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{page.eyebrow}</span>
              <h2 className="mt-4 font-display text-2xl font-semibold leading-tight">{page.title}</h2>
              <p className="mt-4 text-[15px] leading-relaxed text-muted">{page.description}</p>
              <a href={`${prefix}/use-cases/${page.slug}`} className="mt-7 inline-flex items-center gap-2 text-sm font-semibold text-accent hover:text-accent-strong">{c.more} <ArrowRight className="size-4" /></a>
            </Card>
          ))}
        </div>
      </section>
      <CTA t={t} />
    </PageShell>
  );
}
