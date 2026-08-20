import { ArrowRight, Check, LockKeyhole, Search, Sparkles } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Button } from '@/components/ui/button';
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
          <div className="mt-14 grid gap-x-10 gap-y-12 md:grid-cols-3">
            {page.highlights.map((item, index) => {
              const Icon = icons[index % icons.length];
              return (
                <article key={item.title} className="flex h-full flex-col items-center px-2 text-center">
                  <span className="flex size-12 items-center justify-center text-accent">
                    <Icon className="size-7" strokeWidth={1.8} />
                  </span>
                  <h2 className="mt-5 text-balance font-display text-[clamp(20px,2vw,26px)] font-semibold leading-tight">{item.title}</h2>
                  <p className="mt-4 max-w-[360px] text-[15px] leading-relaxed text-muted">{item.body}</p>
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section className="theme-soft-section mx-2 rounded-[28px] bg-[#fbf7ea] sm:mx-4 lg:mx-6">
        <div className="container-x grid gap-14 py-[clamp(64px,8vw,112px)] lg:grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)] lg:items-start lg:gap-[clamp(72px,8vw,128px)]">
          <div className="lg:pt-1">
            <h2 className="max-w-[11ch] text-balance font-[Georgia,'Times_New_Roman','Yu_Mincho','Hiragino_Mincho_ProN',serif] text-[clamp(40px,5vw,68px)] font-normal leading-[1.04] tracking-[-0.045em]">
              {copy.howTitle}
            </h2>
            <p className="mt-7 max-w-[620px] text-[clamp(17px,1.55vw,21px)] leading-[1.65] text-muted">
              {page.intro}
            </p>
          </div>
          <ol className="grid gap-9 lg:pt-1">
            {page.steps.map((step) => (
              <li key={step.title} className="flex items-start gap-5">
                <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full bg-[#4ccd7a] text-[#07130b] shadow-[inset_0_0_0_1px_rgba(0,0,0,0.04)]">
                  <Check className="size-[18px]" strokeWidth={2.75} />
                </span>
                <div>
                  <h3 className="text-[clamp(19px,1.8vw,23px)] font-semibold leading-[1.25]">{step.title}</h3>
                  <p className="mt-2.5 max-w-[620px] text-[clamp(15px,1.2vw,17px)] leading-[1.55] text-muted">{step.body}</p>
                </div>
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
