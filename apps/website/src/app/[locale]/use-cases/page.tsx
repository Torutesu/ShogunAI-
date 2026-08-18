import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import UseCasesPage from '@/app/use-cases/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

const meta = {
  en: ['Use cases — AI memory for knowledge work', 'See how founders, product and engineering teams, and consultants use private AI memory to recall context and complete work.'],
  ja: ['活用事例 — 知識労働のためのAIメモリ', '創業者、プロダクト・開発職、コンサルタントが、プライベートAIメモリで文脈を思い出し仕事を完了する方法を紹介します。'],
  es: ['Casos de uso — Memoria de IA para el trabajo del conocimiento', 'Descubre cómo fundadores, equipos de producto e ingeniería y consultores usan una memoria privada de IA.'],
  de: ['Anwendungsfälle — KI-Gedächtnis für Wissensarbeit', 'Sieh, wie Gründer, Produkt- und Entwicklungsteams sowie Berater privates KI-Gedächtnis nutzen.'],
} as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/use-cases`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/use-cases') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <UseCasesPage searchParams={Promise.resolve({ _locale: locale })} />; }
