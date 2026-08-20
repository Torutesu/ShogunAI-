import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
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
      <CTA t={t} />
    </PageShell>
  );
}
