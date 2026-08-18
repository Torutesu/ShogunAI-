import { ArrowRight, Check, LockKeyhole, Search, Sparkles } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';
import type { MarketingDetail } from '@/lib/marketing-content';
import { siteConfig } from '@/lib/site';

const icons = [Search, Sparkles, LockKeyhole];

const ui = {
  en: { home: 'Home', how: 'How it works', howTitle: 'From context to a useful next step', changes: 'What this changes', changesTitle: 'Spend less time rebuilding context', faq: 'Frequently asked questions', faqTitle: 'Clear answers before you start', explore: 'Explore all', exploreSub: 'See how ShogunAI connects private memory, recall, and execution.', overview: 'View overview' },
  ja: { home: 'ホーム', how: '仕組み', howTitle: '文脈を、役立つ次の一手へ', changes: '変わること', changesTitle: '文脈を組み立て直す時間を減らす', faq: 'よくある質問', faqTitle: '始める前に知っておきたいこと', explore: 'すべての', exploreSub: 'ShogunAIがプライベートな記憶、検索、実行をどうつなぐか確認できます。', overview: '一覧を見る' },
  es: { home: 'Inicio', how: 'Cómo funciona', howTitle: 'Del contexto al siguiente paso útil', changes: 'Qué cambia', changesTitle: 'Dedica menos tiempo a reconstruir contexto', faq: 'Preguntas frecuentes', faqTitle: 'Respuestas claras antes de empezar', explore: 'Explorar', exploreSub: 'Descubre cómo ShogunAI conecta memoria privada, recuperación y ejecución.', overview: 'Ver resumen' },
  de: { home: 'Startseite', how: 'So funktioniert es', howTitle: 'Vom Kontext zum nützlichen nächsten Schritt', changes: 'Was sich ändert', changesTitle: 'Weniger Zeit für das Wiederherstellen von Kontext', faq: 'Häufige Fragen', faqTitle: 'Klare Antworten vor dem Start', explore: 'Alle', exploreSub: 'Erfahre, wie ShogunAI privates Gedächtnis, Abruf und Ausführung verbindet.', overview: 'Übersicht ansehen' },
} as const;

export function MarketingContentPage({ page, section, sectionLabel, t, locale }: { page: MarketingDetail; section: string; sectionLabel: string; t: Dictionary; locale: Locale }) {
  const prefix = `/${locale}`;
  const copy = ui[locale];
  const canonical = `${siteConfig.url}${prefix}/${section}/${page.slug}`;
  const faqSchema = {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: page.faq.map(([question, answer]) => ({
      '@type': 'Question',
      name: question,
      acceptedAnswer: { '@type': 'Answer', text: answer },
    })),
  };
  const webpageSchema = {
    '@context': 'https://schema.org',
    '@type': 'WebPage',
    name: page.title,
    description: page.description,
    url: canonical,
    isPartOf: { '@type': 'WebSite', name: siteConfig.name, url: siteConfig.url },
    about: { '@type': 'SoftwareApplication', name: siteConfig.name, operatingSystem: 'macOS' },
  };

  return (
    <PageShell locale={locale}>
      <JsonLd data={webpageSchema} />
      <JsonLd data={faqSchema} />
      <JsonLd data={breadcrumbSchema([
        { name: copy.home, url: `${siteConfig.url}${prefix}` },
        { name: sectionLabel, url: `${siteConfig.url}${prefix}/${section}` },
        { name: page.title, url: canonical },
      ])} />
      <PageHeader eyebrow={page.eyebrow} title={page.title} sub={page.description} />

      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x">
          <p className="mx-auto max-w-[720px] text-center text-[clamp(18px,2.2vw,22px)] leading-relaxed text-muted">{page.intro}</p>
          <div className="mt-12 grid gap-5 md:grid-cols-3">
            {page.highlights.map((item, index) => {
              const Icon = icons[index % icons.length];
              return (
                <Card key={item.title} className="lift h-full rounded-[24px] p-7">
                  <span className="flex size-10 items-center justify-center rounded-xl bg-sky-soft text-accent"><Icon className="size-5" /></span>
                  <h2 className="mt-5 font-display text-xl font-semibold">{item.title}</h2>
                  <p className="mt-3 text-[15px] leading-relaxed text-muted">{item.body}</p>
                </Card>
              );
            })}
          </div>
        </div>
      </section>

      <section className="border-y border-border bg-cloud/45 py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-10 lg:grid-cols-[0.8fr_1.2fr] lg:items-start">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{copy.how}</p>
            <h2 className="mt-3 font-display text-[clamp(26px,4vw,40px)] font-semibold leading-tight">{copy.howTitle}</h2>
          </div>
          <ol className="grid gap-4">
            {page.steps.map((step, index) => (
              <li key={step.title} className="grid grid-cols-[42px_1fr] gap-4 rounded-[20px] border border-border bg-surface p-5 shadow-[var(--shadow-card)]">
                <span className="flex size-10 items-center justify-center rounded-full bg-accent font-display text-sm font-semibold text-white">{index + 1}</span>
                <div><h3 className="font-display text-lg font-semibold">{step.title}</h3><p className="mt-1.5 text-sm leading-relaxed text-muted">{step.body}</p></div>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-10 lg:grid-cols-2 lg:items-center">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{copy.changes}</p>
            <h2 className="mt-3 font-display text-[clamp(26px,4vw,40px)] font-semibold leading-tight">{copy.changesTitle}</h2>
          </div>
          <ul className="grid gap-3 sm:grid-cols-2">
            {page.outcomes.map((outcome) => (
              <li key={outcome} className="flex items-start gap-3 rounded-xl border border-border bg-surface p-4 text-sm font-medium">
                <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-sky-soft"><Check className="size-3 text-accent" strokeWidth={3} /></span>
                {outcome}
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="border-y border-border bg-cloud/45 py-[clamp(48px,7vw,88px)]">
        <div className="container-x max-w-[820px]">
          <p className="text-center text-xs font-semibold uppercase tracking-[0.08em] text-accent">{copy.faq}</p>
          <h2 className="mt-3 text-center font-display text-[clamp(26px,4vw,40px)] font-semibold">{copy.faqTitle}</h2>
          <div className="mt-8 grid gap-3">
            {page.faq.map(([question, answer]) => (
              <details key={question} className="group rounded-xl border border-border bg-surface px-5 open:shadow-[var(--shadow-card)]">
                <summary className="cursor-pointer list-none py-5 font-semibold [&::-webkit-details-marker]:hidden">{question}</summary>
                <p className="pb-5 text-[15px] leading-relaxed text-muted">{answer}</p>
              </details>
            ))}
          </div>
        </div>
      </section>

      <section className="py-14">
        <div className="container-x flex flex-col items-center justify-between gap-5 rounded-[24px] border border-border bg-surface p-7 text-center shadow-[var(--shadow-card)] sm:flex-row sm:text-left">
          <div><p className="font-display text-xl font-semibold">{copy.explore} {sectionLabel}</p><p className="mt-1 text-sm text-muted">{copy.exploreSub}</p></div>
          <Button asChild variant="secondary"><a href={`${prefix}/${section}`}>{copy.overview} <ArrowRight className="size-4" /></a></Button>
        </div>
      </section>
      <CTA t={t} />
    </PageShell>
  );
}
