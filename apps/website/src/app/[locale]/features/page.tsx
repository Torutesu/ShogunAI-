import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import FeaturesPage from '@/app/features/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

const meta = {
  en: ['Features — Private AI memory, recall, and execution', 'Explore ShogunAI’s local-first AI memory, contextual recall, and execution layer for knowledge work on macOS.'],
  ja: ['機能 — プライベートAIメモリ・文脈検索・実行', 'macOS向けShogunAIのローカルファーストAIメモリ、文脈検索、実行レイヤーを紹介します。'],
  es: ['Funciones — Memoria, recuperación y ejecución privadas con IA', 'Explora la memoria local-first, la recuperación contextual y la capa de ejecución de ShogunAI para macOS.'],
  de: ['Funktionen — Privates KI-Gedächtnis, Abruf und Ausführung', 'Entdecke ShogunAIs local-first KI-Gedächtnis, Kontextsuche und Ausführungsebene für macOS.'],
} as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/features`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/features') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <FeaturesPage searchParams={Promise.resolve({ _locale: locale })} />; }
