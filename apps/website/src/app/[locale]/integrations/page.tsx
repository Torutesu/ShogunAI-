import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import IntegrationsPage from '@/app/integrations/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';
const meta = { en: ['Integrations — Connect AI memory to your work tools', 'Connect private work memory and execution across 20+ tools.'], ja: ['連携 — AIメモリを仕事のツールにつなぐ', 'ShogunAIのプライベートな仕事の記憶と実行を、20以上のツールにつなぎます。'], es: ['Integraciones — Conecta la memoria de IA con tus herramientas', 'Conecta memoria privada y ejecución entre más de 20 herramientas.'], de: ['Integrationen — Verbinde KI-Gedächtnis mit deinen Tools', 'Verbinde privates Arbeitsgedächtnis und Ausführung über mehr als 20 Tools.'] } as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/integrations`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/integrations') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <IntegrationsPage searchParams={Promise.resolve({ _locale: locale })} />; }
