import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
import { AppDemo } from '@/components/AppDemo';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { getFeaturePages } from '@/lib/marketing-content';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Features — Private AI memory, recall, and execution',
  description: 'Explore ShogunAI’s local-first AI memory, contextual recall, and execution layer for knowledge work on macOS.',
  alternates: { canonical: '/en/features', languages: localizedAlternates('/features') },
};

const copy = {
  en: { eyebrow: 'Product features', title: 'One context layer from memory to action', sub: 'ShogunAI connects three parts of knowledge work that are usually separated: remembering what happened, retrieving the right context, and completing the next step.', more: 'Learn more' },
  ja: { eyebrow: 'プロダクト機能', title: '記憶から実行までを、一つの文脈レイヤーで', sub: '起きたことを記憶し、必要な文脈を取り出し、次の作業を完了する。分断されがちな知識労働の三つの工程をShogunAIがつなぎます。', more: '詳しく見る' },
  es: { eyebrow: 'Funciones del producto', title: 'Una capa de contexto desde la memoria hasta la acción', sub: 'ShogunAI conecta tres partes del trabajo del conocimiento: recordar lo ocurrido, recuperar el contexto adecuado y completar el siguiente paso.', more: 'Más información' },
  de: { eyebrow: 'Produktfunktionen', title: 'Eine Kontextebene vom Gedächtnis bis zur Handlung', sub: 'ShogunAI verbindet drei Teile der Wissensarbeit: Geschehenes erinnern, passenden Kontext abrufen und den nächsten Schritt erledigen.', more: 'Mehr erfahren' },
} as const;

export default async function FeaturesPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const pages = getFeaturePages(locale);
  const c = copy[locale];
  const prefix = `/${locale}`;
  return (
    <PageShell locale={locale}>
      <JsonLd data={breadcrumbSchema([{ name: 'Home', url: `${siteConfig.url}${prefix}` }, { name: c.eyebrow, url: `${siteConfig.url}${prefix}/features` }])} />
      <PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.sub} />
      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-6 lg:grid-cols-3">
          {pages.map((page, index) => (
            <Card key={page.slug} className="lift flex h-full flex-col rounded-[26px] p-7">
              <span className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">0{index + 1} · {page.eyebrow}</span>
              <h2 className="mt-4 font-display text-2xl font-semibold leading-tight">{page.title}</h2>
              <p className="mt-4 text-[15px] leading-relaxed text-muted">{page.description}</p>
              <a href={`${prefix}/features/${page.slug}`} className="mt-7 inline-flex items-center gap-2 text-sm font-semibold text-accent hover:text-accent-strong">{c.more} <ArrowRight className="size-4" /></a>
            </Card>
          ))}
        </div>
      </section>
      {/* The app itself, running with its sample fixture — not a screenshot of it. */}
      <section className="border-y border-border bg-cloud/45 py-[clamp(48px,7vw,88px)]">
        <div className="container-x">
          <AppDemo label={t.featureSpec.demoLabel} title={t.featureSpec.demoTitle} hint={t.featureSpec.demoHint} openLabel={t.featureSpec.demoOpen} />
        </div>
      </section>

      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[760px] text-center">
            <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.featureSpec.eyebrow}</p>
            <h2 className="mt-3.5 font-display text-[clamp(26px,4vw,40px)] font-semibold leading-[1.12] tracking-[-0.02em] text-balance">{t.featureSpec.title}</h2>
            <p className="mt-4 text-[16px] leading-relaxed text-muted">{t.featureSpec.sub}</p>
          </div>
          <div className="mt-10 grid gap-5">
            {t.featureSpec.groups.map((group) => (
              <div key={group.name} className="rounded-[26px] border border-border bg-surface p-6 sm:p-7">
                <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-accent">{group.name}</h3>
                <dl className="mt-4 grid gap-4">
                  {group.items.map((item) => (
                    <div key={item.k} className="grid gap-1.5 border-t border-border pt-4 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.5fr)_minmax(0,1.3fr)] lg:gap-6">
                      <dt className="text-[15px] font-semibold leading-snug">{item.k}</dt>
                      <dd className="text-[14px] leading-relaxed text-muted">{item.v}</dd>
                      <dd className="rounded-[14px] bg-cloud px-3.5 py-2.5 text-[13px] leading-relaxed text-muted">
                        <span className="mr-1.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-accent">{t.featureSpec.ruleLabel}</span>
                        {item.r}
                      </dd>
                    </div>
                  ))}
                </dl>
              </div>
            ))}
          </div>
        </div>
      </section>

      <CTA t={t} />
    </PageShell>
  );
}
