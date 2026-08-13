import type { Metadata } from 'next';

import { LicenseKeyCard } from '@/components/billing/LicenseKeyCard';
import { Footer } from '@/components/sections/Footer';
import { Nav } from '@/components/sections/Nav';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { findLicenseBySubscription } from '@/db/billing-queries';
import { PLANS, isPlanId, priceLine, type Interval } from '@/lib/pricing';
import { stripe, stripeSecretKey } from '@/lib/stripe';

// The licence key is on this page. Never index it, never cache it.
export const metadata: Metadata = { robots: { index: false, follow: false }, title: 'Subscription' };
export const dynamic = 'force-dynamic';

/**
 * /billing/success — step 4 of the flow (issue #8): Stripe redirects here after Checkout.
 *
 * The one job that cannot be done anywhere else: hand the buyer their licence key, which is what
 * unlocks the Mac app. The webhook that mints it may not have landed yet (Stripe fires the
 * redirect and the webhook in parallel), so a not-yet-provisioned licence shows a refresh state
 * instead of an error — the money is taken either way and telling someone "not found" here is
 * the worst possible first minute of a paid relationship.
 */
export default async function BillingSuccessPage({
  searchParams,
}: {
  searchParams: Promise<{ session_id?: string }>;
}) {
  const { session_id: sessionId } = await searchParams;

  let licenseKey: string | null = null;
  let planLine: string | null = null;
  let pending = false;

  if (sessionId && stripeSecretKey()) {
    try {
      const session = await stripe().checkout.sessions.retrieve(sessionId);
      const subId =
        typeof session.subscription === 'string' ? session.subscription : session.subscription?.id;
      if (subId) {
        const license = await findLicenseBySubscription(subId);
        licenseKey = license?.licenseKey ?? null;
        pending = !licenseKey;
      }
      const plan = session.metadata?.plan;
      const interval = session.metadata?.interval;
      if (isPlanId(plan) && (interval === 'monthly' || interval === 'annual')) {
        planLine = `${PLANS[plan].name} — ${priceLine(plan, interval as Interval)}`;
      }
    } catch {
      // A stale or foreign session id lands in the generic state below.
      pending = false;
    }
  }

  return (
    <>
      <Nav />
      <main id="top" className="py-[clamp(56px,9vw,112px)]">
        <div className="container-x max-w-[640px]">
          {licenseKey ? (
            <LicenseKeyCard licenseKey={licenseKey} planLine={planLine} />
          ) : (
            <Card className="grid gap-4 p-8 text-center">
              <h1 className="font-display text-2xl font-semibold">
                {pending ? 'Setting up your subscription' : 'Thanks — you’re subscribed'}
              </h1>
              <p className="text-muted">
                {pending
                  ? 'Your payment went through. Your licence key appears here within a few seconds — refresh this page.'
                  : 'Open ShogunAI and go to Settings → Plan & billing to activate this Mac with the licence key from your receipt email.'}
              </p>
              <div className="flex justify-center gap-3">
                {pending && (
                  <Button asChild>
                    <a href={sessionId ? `/billing/success?session_id=${encodeURIComponent(sessionId)}` : '/billing/success'}>
                      Refresh
                    </a>
                  </Button>
                )}
                <Button asChild variant="secondary">
                  <a href="/">Back to home</a>
                </Button>
              </div>
            </Card>
          )}
        </div>
      </main>
      <Footer />
    </>
  );
}
