import { Action } from '@/components/sections/Action';
import { Campaign } from '@/components/sections/Campaign';
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
import { Stats } from '@/components/sections/Stats';
import { Testimonials } from '@/components/sections/Testimonials';
import { UseCases } from '@/components/sections/UseCases';
import { countParticipants, findByRefCode } from '@/db/queries';
import { currentTier, isValidRefCode, maskEmail } from '@/lib/referral';
import { countQualifiedReferrals } from '@/lib/service';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

// Baseline for the "468+" scarcity counter. Real signups are added ON TOP of
// this — but only in production with WAITLIST_LIVE_COUNT enabled. In dev /
// preview / verification the counter always reads exactly the base, so test
// signups never leak into the public number.
const WAITLIST_BASE = 468;

async function joinedCount(): Promise<number> {
  if (process.env.WAITLIST_LIVE_COUNT !== 'true') return WAITLIST_BASE;
  try {
    return WAITLIST_BASE + (await countParticipants());
  } catch {
    return WAITLIST_BASE; // DB not ready — show the base
  }
}

async function inviteContext(ref?: string) {
  if (!ref || !isValidRefCode(ref)) return null;
  try {
    const inviter = await findByRefCode(ref);
    if (!inviter) return null;
    const count = await countQualifiedReferrals(ref);
    return { inviter: maskEmail(inviter.email), tier: currentTier(count) };
  } catch {
    return null; // DB not ready — degrade gracefully
  }
}

export default async function Home({ searchParams }: { searchParams: Promise<{ ref?: string }> }) {
  const { ref } = await searchParams;
  const [{ t }, invite, joined] = await Promise.all([getI18n(), inviteContext(ref), joinedCount()]);

  return (
    <>
      <Campaign t={t} />
      <Nav />
      <main id="top">
        <Hero t={t} refCode={ref} invite={invite} joined={joined} />
        <Marquee t={t} />
        <Memory t={t} />
        <Action t={t} />
        <UseCases t={t} />
        <How t={t} />
        <Stats t={t} />
        <Testimonials t={t} />
        <Privacy t={t} />
        <Pricing pricing={t.pricing} />
        <FAQ t={t} />
        <CTA t={t} refCode={ref} />
      </main>
      <Footer />
    </>
  );
}
