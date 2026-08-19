import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
import { PageHeader, PageShell } from '@/components/PageShell';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { localizedAlternates } from '@/lib/site';

export const metadata: Metadata = { title: 'Compare AI memory tools', description: 'Compare ShogunAI with knowledge bases and AI memory tools by context coverage, privacy model, recall, and execution.', alternates: { canonical: '/en/compare', languages: localizedAlternates('/compare') } };

const content = {
  en: { eyebrow: 'Compare', title: 'Choose the right context layer for your work', sub: 'ShogunAI is designed for an individual’s private work memory. These guides explain how that model differs from notes, company knowledge, and enterprise search.', read: 'Read comparison', note: 'Comparison pages explain product models and trade-offs. Third-party features and pricing can change; verify current details with each provider.', items: ['Personal work memory and execution compared with a shared documentation workspace.', 'Local-first work context compared with an AI-native notes and knowledge product.', 'An individual context layer compared with enterprise search and company knowledge.'] },
  ja: { eyebrow: '製品比較', title: '自分の仕事に合う文脈レイヤーを選ぶ', sub: 'ShogunAIは、個人のプライベートな仕事の記憶を中心に設計されています。ノート、社内ナレッジ、エンタープライズ検索との違いを比較します。', read: '比較記事を読む', note: '比較ページは製品モデルと選択時の論点を説明するものです。他社製品の機能・料金は変更される可能性があるため、最新情報は各社の公式情報をご確認ください。', items: ['個人の仕事の記憶・実行と、共有ドキュメント環境を比較します。', 'ローカルファーストな仕事の文脈と、AIネイティブなノート・ナレッジ製品を比較します。', '個人の文脈レイヤーと、企業向け検索・社内ナレッジを比較します。'] },
  es: { eyebrow: 'Comparar', title: 'Elige la capa de contexto adecuada para tu trabajo', sub: 'ShogunAI se centra en la memoria privada individual. Estas guías la comparan con notas, conocimiento empresarial y búsqueda corporativa.', read: 'Leer comparación', note: 'Las funciones y precios de terceros pueden cambiar. Verifica los detalles actuales con cada proveedor.', items: ['Memoria y ejecución personal frente a documentación compartida.', 'Contexto local-first frente a notas y conocimiento nativos de IA.', 'Contexto individual frente a búsqueda y conocimiento empresarial.'] },
  de: { eyebrow: 'Vergleichen', title: 'Wähle die richtige Kontextebene für deine Arbeit', sub: 'ShogunAI ist für das private Arbeitsgedächtnis Einzelner konzipiert. Diese Leitfäden vergleichen es mit Notizen, Unternehmenswissen und Enterprise Search.', read: 'Vergleich lesen', note: 'Funktionen und Preise Dritter können sich ändern. Prüfe aktuelle Angaben beim jeweiligen Anbieter.', items: ['Persönliches Gedächtnis und Ausführung gegenüber geteilter Dokumentation.', 'Local-first Kontext gegenüber KI-nativen Notizen und Wissen.', 'Individuelle Kontextebene gegenüber Enterprise Search und Unternehmenswissen.'] },
} as const;
const names = ['ShogunAI vs Notion', 'ShogunAI vs Mem', 'ShogunAI vs Glean'] as const;
const slugs = ['shogunai-vs-notion', 'shogunai-vs-mem', 'shogunai-vs-glean'] as const;

export default async function ComparePage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale } = await getI18n(localeOverride);
  const c = content[locale]; const prefix = `/${locale}`;
  return <PageShell locale={locale}><PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.sub} /><section className="py-[clamp(48px,7vw,88px)]"><div className="container-x grid gap-6 md:grid-cols-3">{names.map((name, index) => <Card key={name} className="lift flex h-full flex-col rounded-[26px] p-7"><h2 className="font-display text-2xl font-semibold">{name}</h2><p className="mt-4 text-[15px] leading-relaxed text-muted">{c.items[index]}</p><a href={`${prefix}/blog/${slugs[index]}`} className="mt-7 inline-flex items-center gap-2 text-sm font-semibold text-accent">{c.read} <ArrowRight className="size-4" /></a></Card>)}</div><div className="mx-auto mt-12 max-w-[760px] rounded-[24px] border border-border bg-cloud/50 p-6 text-center text-sm leading-relaxed text-muted">{c.note}</div></section></PageShell>;
}
