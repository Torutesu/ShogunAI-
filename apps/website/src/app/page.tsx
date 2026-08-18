import { Action } from '@/components/sections/Action';
import { CTA } from '@/components/sections/CTA';
import { FAQ } from '@/components/sections/FAQ';
import { Footer } from '@/components/sections/Footer';
import { Hero } from '@/components/sections/Hero';
import { How } from '@/components/sections/How';
import { Marquee } from '@/components/sections/Marquee';
import { Memory } from '@/components/sections/Memory';
import { Nav } from '@/components/sections/Nav';
import { Pricing } from '@/components/sections/Pricing';
import { Privacy } from '@/components/sections/Privacy';
import { UseCases } from '@/components/sections/UseCases';
import { getI18n } from '@/i18n/server';
import type { Metadata } from 'next';
import type { Locale } from '@/i18n/config';

export const metadata: Metadata = {
  alternates: {
    canonical: '/',
    languages: { en: '/en', ja: '/ja', es: '/es', de: '/de', 'x-default': '/' },
  },
};

export default async function Home({ localeOverride }: { localeOverride?: Locale } = {}) {
  const { locale, t } = await getI18n(localeOverride);
  // The initial cohort was collected before this Supabase-backed form. New
  // registrations are counted live from the database on top of that cohort.
  // The live count is refreshed on the client. Do not make the page render
  // depend on Supabase availability: the marketing site must always load.
  const participantCount = Math.max(0, Number(process.env.WAITLIST_IMPORTED_COUNT ?? 485));

  return (
    <>
      <link rel="preload" as="image" href="/optimized/shogunai-hero-kyoto-v3.jpg" fetchPriority="high" />
      <div className="sticky top-0 z-50">
        <Nav localeOverride={locale} />
      </div>
      <main id="top" lang={locale}>
        <Hero t={t} participantCount={participantCount} />
        <Marquee t={t} />
        <Memory t={t} locale={locale} />
        <Action t={t} locale={locale} />
        <UseCases t={t} locale={locale} />
        <How t={t} locale={locale} />
        <Privacy t={t} locale={locale} />
        <Pricing pricing={t.pricing} />
        <FAQ t={t} />
        <CTA t={t} />
      </main>
      <Footer localeOverride={locale} />
    </>
  );
}
