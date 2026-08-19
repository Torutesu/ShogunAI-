import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import ComparePage from '@/app/compare/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';
const meta = { en: ['Compare AI memory tools', 'Compare ShogunAI with knowledge bases and AI memory tools.'], ja: ['AIメモリ製品を比較', '文脈の範囲、プライバシー、検索、実行の観点からShogunAIと他の製品モデルを比較します。'], es: ['Comparar herramientas de memoria de IA', 'Compara ShogunAI con bases de conocimiento y herramientas de memoria de IA.'], de: ['KI-Gedächtnis-Tools vergleichen', 'Vergleiche ShogunAI mit Wissensdatenbanken und KI-Gedächtnis-Tools.'] } as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/compare`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/compare') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <ComparePage searchParams={Promise.resolve({ _locale: locale })} />; }
