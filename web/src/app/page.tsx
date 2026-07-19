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
import { Stats } from '@/components/sections/Stats';
import { Testimonials } from '@/components/sections/Testimonials';
import { findByRefCode } from '@/db/queries';
import { currentTier, isValidRefCode, maskEmail } from '@/lib/referral';
import { countQualifiedReferrals } from '@/lib/service';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

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
  const [{ t }, invite] = await Promise.all([getI18n(), inviteContext(ref)]);

  return (
    <>
      <Nav />
      <main id="top">
        <Hero t={t} refCode={ref} invite={invite} />
        <Marquee t={t} />
        <Memory t={t} />
        <Action t={t} />
        <How t={t} />
        <Stats t={t} />
        <Testimonials t={t} />
        <Pricing t={t} />
        <FAQ t={t} />
        <CTA t={t} refCode={ref} />
      </main>
      <Footer />
    </>
  );
}
