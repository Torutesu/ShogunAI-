import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import SecurityPage from '@/app/security/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';
const meta = { en: ['Privacy & security — Local-first AI memory', 'Learn how ShogunAI keeps memory local-first, supports BYOK, and uses approval gates.'], ja: ['プライバシー・セキュリティ — ローカルファーストAIメモリ', 'ShogunAIのローカルファーストな記憶、BYOK、重要操作の承認について説明します。'], es: ['Privacidad y seguridad — Memoria de IA local-first', 'Descubre la memoria local-first, BYOK y los controles de aprobación de ShogunAI.'], de: ['Datenschutz & Sicherheit — Local-first KI-Gedächtnis', 'Erfahre mehr über local-first Gedächtnis, BYOK und Freigaben in ShogunAI.'] } as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/security`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/security') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <SecurityPage searchParams={Promise.resolve({ _locale: locale })} />; }
