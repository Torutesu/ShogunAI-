import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import PricingPage from '@/app/pricing/page';
import { isLocale, locales } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';
const meta = { en: ['Pricing — Standard and Pro plans', 'Private AI memory starts at $49 per month billed annually, with Pro for unlimited recall and autonomous execution.'], ja: ['料金 — Standard・Proプラン', 'プライベートAIメモリは年払いで月額49ドルから。Proで無制限の検索と自律実行を利用できます。'], es: ['Precios — Planes Standard y Pro', 'La memoria privada empieza en 49 USD al mes con facturación anual; Pro añade recuperación ilimitada y ejecución autónoma.'], de: ['Preise — Standard und Pro', 'Privates KI-Gedächtnis beginnt bei 49 USD pro Monat bei jährlicher Abrechnung; Pro bietet unbegrenzten Abruf und autonome Ausführung.'] } as const;
export function generateStaticParams() { return locales.map((locale) => ({ locale })); }
export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> { const { locale } = await params; if (!isLocale(locale)) return {}; const [title, description] = meta[locale]; const url = `${siteConfig.url}/${locale}/pricing`; return { title, description, alternates: { canonical: url, languages: localizedAlternates('/pricing') }, openGraph: { title, description, url }, twitter: { card: 'summary_large_image', title, description } }; }
export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <PricingPage searchParams={Promise.resolve({ _locale: locale })} />; }
