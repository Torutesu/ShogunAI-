import type { Metadata } from 'next';
import { PageShell } from '@/components/PageShell';
import { Pricing } from '@/components/sections/Pricing';
import { FAQ } from '@/components/sections/FAQ';
import { CTA } from '@/components/sections/CTA';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { localizedAlternates } from '@/lib/site';

export const metadata: Metadata = { title: 'Pricing — Standard and Pro plans', description: 'Start with private AI memory and natural-language recall, then choose Pro for unlimited memory, 20+ tools, and autonomous execution.', alternates: { canonical: '/en/pricing', languages: localizedAlternates('/pricing') } };

const copy = {
  en: ['Pricing', 'Start with memory. Upgrade when you are ready to act.', 'Choose Standard for private memory and everyday execution, or Pro for unlimited recall, every integration, and autonomous actions.'],
  ja: ['料金', 'まずは記憶から。実行が必要になったらアップグレード。', 'プライベートメモリと日々の実行にはStandard、無制限の検索・全連携・自律実行にはProを選べます。'],
  es: ['Precios', 'Empieza con memoria. Mejora cuando estés listo para actuar.', 'Elige Standard para memoria privada y ejecución diaria, o Pro para recuperación ilimitada, todas las integraciones y acciones autónomas.'],
  de: ['Preise', 'Starte mit Erinnerung. Upgrade, wenn du handeln möchtest.', 'Wähle Standard für privates Gedächtnis und tägliche Ausführung oder Pro für unbegrenzten Abruf, alle Integrationen und autonome Aktionen.'],
} as const;

export default async function PricingPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const [eyebrow, title, sub] = copy[locale];
  return <PageShell locale={locale}><Pricing pricing={t.pricing} heading={{ eyebrow, title, sub }} headingLevel="h1" /><FAQ t={t} /><CTA t={t} /></PageShell>;
}
