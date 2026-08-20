import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import MarketPage from '@/app/market/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

const meta = {
  en: ['Market — What investors are saying about memory & context', 'Sourced public commentary from a16z, Emergence Capital, Kindred Ventures and Wing on memory, context, and agents as coworkers.'],
  ja: ['市場 — 投資家がメモリと文脈について語っていること', 'a16z、Emergence Capital、Kindred Ventures、Wing の公開コメントを出典つきで整理し、メモリと文脈のレイヤーがどこへ向かっているかをまとめました。'],
  es: ['Mercado — Qué dicen los inversores sobre la memoria y el contexto', 'Comentarios públicos con fuentes de a16z, Emergence Capital, Kindred Ventures y Wing sobre memoria, contexto y agentes como colegas.'],
  de: ['Markt — Was Investoren über Memory und Kontext sagen', 'Belegte öffentliche Aussagen von a16z, Emergence Capital, Kindred Ventures und Wing zu Memory, Kontext und Agenten als Kollegen.'],
} as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/market`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/market') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <MarketPage searchParams={Promise.resolve({ _locale: locale })} />; }
