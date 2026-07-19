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
import { Stats } from '@/components/sections/Stats';
import { Testimonials } from '@/components/sections/Testimonials';
import { UseCases } from '@/components/sections/UseCases';
import { countParticipants, findByRefCode } from '@/db/queries';
import { currentTier, isValidRefCode, maskEmail } from '@/lib/referral';
import { countQualifiedReferrals } from '@/lib/service';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

// Marketing seed for the scarcity counter; real signups are added on top.
const WAITLIST_SEED = 900;

async function joinedCount(): Promise<number> {
  try {
    return WAITLIST_SEED + (await countParticipants());
  } catch {
    return WAITLIST_SEED; // DB not ready — show the seed
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
        <Pricing t={t} />
        <FAQ t={t} />
        <CTA t={t} refCode={ref} />
      </main>
      <Footer />
    </>
  );
}
