import { notFound } from 'next/navigation';
import AboutPage from '@/app/about/page';
import { isLocale, locales } from '@/i18n/config';

export const dynamic = 'force-dynamic';

export function generateStaticParams() { return locales.map((locale) => ({ locale })); }

export default async function Page({ params }: { params: Promise<{ locale: string }> }) { const { locale } = await params; if (!isLocale(locale)) notFound(); return <AboutPage searchParams={Promise.resolve({ _locale: locale })} />; }
