'use client';

import { Check } from 'lucide-react';
import { useState } from 'react';
import posthog from 'posthog-js';
import { Reveal } from '@/components/animations/Reveal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

type PlanData = {
  name: string;
  price: string;
  per: string;
  desc: string;
  points: readonly string[];
  cta: string;
  badge?: string;
};

/**
 * Billing is behind a build flag (issue #8). While the product is invite-only the CTA still
 * feeds the waitlist; flip `NEXT_PUBLIC_BILLING_ENABLED=1` and the same button opens Stripe
 * Checkout. One switch, so launch day is a redeploy rather than a code change.
 */
const BILLING_ENABLED = process.env.NEXT_PUBLIC_BILLING_ENABLED === '1';

function Plan({
  plan,
  planId,
  featured,
}: {
  plan: PlanData;
  planId: 'standard' | 'pro';
  featured?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  /**
   * The headline price is the annual one, so that is what this buys. The client sends the plan
   * name only — the Price ID lives on the server (issue #8 セキュリティ). Monthly billing is
   * reachable from the app's settings and from the Stripe portal.
   */
  const startCheckout = async () => {
    setBusy(true);
    setErr('');
    try {
      const res = await fetch('/api/stripe/checkout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plan: planId, interval: 'annual', source: 'lp' }),
      });
      const data: unknown = await res.json();
      const url = res.ok && typeof data === 'object' && data && 'url' in data ? String(data.url) : '';
      if (!url) throw new Error('no url');
      window.location.assign(url);
    } catch {
      setBusy(false);
      setErr('Could not open checkout. Please try again.');
    }
  };

  return (
    <Card
      className={`lift relative flex flex-col p-7 ${featured ? 'border-accent shadow-[0_12px_40px_rgba(0,166,244,0.14)]' : ''}`}
    >
      {plan.badge && (
        <Badge dot className="bg-sky absolute -top-3.5 left-7">
          {plan.badge}
        </Badge>
      )}
      <div className="font-display text-lg font-semibold">{plan.name}</div>
      <div className="my-2 flex items-baseline gap-1.5">
        <span className="font-display text-[40px] font-semibold tracking-[-0.02em]">
          {plan.price}
        </span>
        <span className="text-muted">{plan.per}</span>
      </div>
      <p className="text-muted text-sm">{plan.desc}</p>
      <ul className="my-6 grid gap-3">
        {plan.points.map((p) => (
          <li key={p} className="text-ink flex items-start gap-3 text-sm">
            <span className="bg-sky-soft mt-0.5 flex size-[18px] shrink-0 items-center justify-center rounded-full">
              <Check className="text-accent size-3" strokeWidth={3} />
            </span>
            {p}
          </li>
        ))}
      </ul>
      {BILLING_ENABLED ? (
        <>
          <Button
            variant={featured ? 'primary' : 'secondary'}
            className="mt-auto w-full"
            disabled={busy}
            onClick={() => {
              posthog.capture('pricing_cta_clicked', {
                plan: plan.name,
                featured: !!featured,
                destination: 'checkout',
              });
              void startCheckout();
            }}
          >
            {busy ? '…' : plan.cta}
          </Button>
          {err && <p className="mt-2 text-center text-sm text-red-500">{err}</p>}
        </>
      ) : (
        <Button
          asChild
          variant={featured ? 'primary' : 'secondary'}
          className="mt-auto w-full"
          onClick={() =>
            posthog.capture('pricing_cta_clicked', {
              plan: plan.name,
              featured: !!featured,
              destination: 'waitlist',
            })
          }
        >
          <a href="#get-started">{plan.cta}</a>
        </Button>
      )}
    </Card>
  );
}

export function Pricing({ t }: { t: Dictionary }) {
  return (
    <section id="pricing" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[44ch] text-center">
          <p className="text-accent text-xs font-semibold tracking-[0.08em] uppercase">
            {t.pricing.eyebrow}
          </p>
          <h2 className="font-display mt-3.5 text-[clamp(30px,4vw,44px)] leading-[1.1] font-semibold tracking-[-0.015em] text-balance">
            {t.pricing.title}
          </h2>
          <p className="text-muted mt-4 text-[17px]">{t.pricing.sub}</p>
        </Reveal>
        <div className="mx-auto grid max-w-[780px] gap-6 md:grid-cols-2">
          <Reveal>
            <Plan plan={t.pricing.standard} planId="standard" />
          </Reveal>
          <Reveal delay={0.08}>
            <Plan plan={t.pricing.pro} planId="pro" featured />
          </Reveal>
        </div>
      </div>
    </section>
  );
}
