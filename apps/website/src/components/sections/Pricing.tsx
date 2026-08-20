'use client';

import { Check, Command, Languages, Layers, ListChecks, Search, Sparkles, Sunrise, Video, Zap } from 'lucide-react';
import { useState } from 'react';
import { Reveal } from '@/components/animations/Reveal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

type Pricing = Dictionary['pricing'];
type PlanData = { name: string; annual: string; monthly: string; points: readonly string[]; cta: string; badge?: string };
type PlanId = 'standard' | 'pro';

/**
 * Send the visitor to Stripe Checkout for this plan × interval.
 *
 * The plan *name* goes over the wire, never a Price ID — the server resolves the price so a
 * tampered request cannot buy Pro at the Standard price (issue #8 セキュリティ). Any failure falls
 * back to the download section rather than leaving a dead button: an environment with no prices
 * set answers 503 (`billingReady()`), and a visitor who cannot buy should still get the app.
 *
 * Asking the route at click time rather than reading `billingReady()` when the page renders is
 * deliberate: these pages are statically generated, so a build-time answer would be baked into
 * the HTML — and on a Workers deploy the environment is not populated at build time at all,
 * which would leave the LP permanently unable to sell.
 */
async function startCheckout(plan: PlanId, interval: 'monthly' | 'annual'): Promise<void> {
  try {
    const res = await fetch('/api/stripe/checkout', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ plan, interval, source: 'lp' }),
    });
    const body: unknown = await res.json();
    const url =
      res.ok && typeof body === 'object' && body !== null && 'url' in body
        ? (body as { url?: unknown }).url
        : null;
    if (typeof url === 'string' && url.startsWith('https://')) {
      window.location.href = url;
      return;
    }
  } catch {
    // Fall through to the download section.
  }
  document.getElementById('get-started')?.scrollIntoView({ behavior: 'smooth' });
}

function BillingToggle({
  annual,
  onChange,
  b,
}: {
  annual: boolean;
  onChange: (v: boolean) => void;
  b: Pricing['billing'];
}) {
  return (
    <div role="group" aria-label={b.annualNote} className="relative z-10 mx-auto mb-8 flex w-fit items-center gap-1 rounded-full border border-border bg-cloud p-1 shadow-sm">
      <button
        type="button"
        aria-pressed={!annual}
        onClick={() => onChange(false)}
        className={`min-h-11 touch-manipulation rounded-full px-4 py-2 text-sm font-medium transition-colors ${
          !annual ? 'bg-surface text-ink shadow-[var(--shadow-card)]' : 'text-muted hover:text-ink'
        }`}
      >
        {b.monthly}
      </button>
      <button
        type="button"
        aria-pressed={annual}
        onClick={() => onChange(true)}
        className={`flex min-h-11 touch-manipulation items-center gap-1.5 rounded-full px-4 py-2 text-sm font-medium transition-colors ${
          annual ? 'bg-surface text-ink shadow-[var(--shadow-card)]' : 'text-muted hover:text-ink'
        }`}
      >
        {b.annual}
        <span className="rounded-full bg-sky-soft px-1.5 py-0.5 text-[11px] font-semibold text-accent-strong">
          {b.save}
        </span>
      </button>
    </div>
  );
}

function Plan({
  plan,
  planId,
  annual,
  b,
  featured,
}: {
  plan: PlanData;
  planId: PlanId;
  annual: boolean;
  b: Pricing['billing'];
  featured?: boolean;
}) {
  const [starting, setStarting] = useState(false);
  const price = annual ? plan.annual : plan.monthly;
  return (
    <Card
      className={`lift relative flex flex-col p-7 ${featured ? 'border-accent shadow-[0_12px_40px_rgba(0,76,252,0.16)]' : ''}`}
    >
      {plan.badge && (
        <Badge dot className="absolute -top-3.5 left-7 bg-sky">
          {plan.badge}
        </Badge>
      )}
      <div className="font-display text-lg font-semibold">{plan.name}</div>
      <div className="my-2 flex items-baseline gap-1.5">
        <span className="font-display text-[40px] font-semibold tracking-[-0.02em] tabular-nums">{price}</span>
        <span className="text-muted">{b.perMonth}</span>
      </div>
      <p className="text-sm text-muted">{annual ? b.annualNote : b.monthlyNote}</p>
      <ul className="my-6 grid gap-3">
        {plan.points.map((p) => (
          <li key={p} className="flex items-start gap-3 text-sm text-ink">
            <span className="mt-0.5 flex size-[18px] shrink-0 items-center justify-center rounded-full bg-sky-soft">
              <Check className="size-3 text-accent" strokeWidth={3} />
            </span>
            {p}
          </li>
        ))}
      </ul>
      <Button
        variant={featured ? 'primary' : 'secondary'}
        className="mt-auto w-full"
        disabled={starting}
        onClick={() => {
          setStarting(true);
          void startCheckout(planId, annual ? 'annual' : 'monthly').finally(() => setStarting(false));
        }}
      >
        {plan.cta}
      </Button>
    </Card>
  );
}

/** Icons follow the item order in the dictionary; extra items fall back to a neutral mark. */
const BUNDLE_ICONS = [Video, Search, Layers, Sunrise, Zap, Command, Languages];

function Bundle({ b }: { b: Pricing['bundle'] }) {
  return (
    <Reveal className="mt-[clamp(40px,6vw,72px)]">
      <div className="mx-auto max-w-[980px] rounded-[26px] border border-border bg-cloud/45 p-7 sm:p-9">
        <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted">{b.label}</p>
        <h3 className="mt-3 max-w-[34ch] font-display text-[clamp(22px,3vw,32px)] font-semibold leading-[1.14] tracking-[-0.02em] text-balance">
          {b.title}
        </h3>
        <p className="mt-3 max-w-[62ch] text-[15px] leading-relaxed text-muted">{b.sub}</p>
        <ul className="mt-7 grid gap-3 sm:grid-cols-2">
          {b.items.map((item, index) => {
            const Icon = BUNDLE_ICONS[index] ?? ListChecks;
            return (
              <li
                key={item.name}
                className={`flex items-start gap-3.5 rounded-[18px] border border-border bg-surface p-4 ${index === b.items.length - 1 && b.items.length % 2 === 1 ? 'sm:col-span-2' : ''}`}
              >
                <span className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-sky-soft text-accent">
                  <Icon className="size-4" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-semibold">{item.name}</span>
                  <span className="mt-1 block text-[13px] leading-relaxed text-muted">{item.note}</span>
                </span>
              </li>
            );
          })}
        </ul>
        <p className="mt-6 flex items-start gap-2.5 text-[13px] leading-relaxed text-muted">
          <Sparkles className="mt-0.5 size-4 shrink-0 text-accent" />
          {b.footnote}
        </p>
      </div>
    </Reveal>
  );
}

export function Pricing({ pricing, heading, headingLevel = 'h2' }: { pricing: Pricing; heading?: { eyebrow: string; title: string; sub: string }; headingLevel?: 'h1' | 'h2' }) {
  const [annual, setAnnual] = useState(true); // default: annual (recommended)
  const display = heading ?? pricing;
  const Heading = headingLevel;
  return (
    <section id="pricing" className="relative scroll-mt-24 py-[clamp(48px,7vw,88px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-7 max-w-[48ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{display.eyebrow}</p>
          <Heading className="pricing-title mt-3.5 font-display text-[clamp(28px,5.5vw,48px)] font-semibold leading-[1.08] tracking-[-0.02em] text-balance">
            {display.title}
          </Heading>
          <p className="pricing-sub mx-auto mt-4 max-w-[58ch] text-[17px] leading-relaxed text-muted">{display.sub}</p>
        </Reveal>
        <Reveal>
          <BillingToggle annual={annual} onChange={setAnnual} b={pricing.billing} />
        </Reveal>
        <div className="mx-auto grid max-w-[780px] gap-6 md:grid-cols-2">
          <Reveal>
            <Plan
              plan={pricing.standard}
              planId="standard"
              annual={annual}
              b={pricing.billing}
            />
          </Reveal>
          <Reveal delay={0.08}>
            <Plan
              plan={pricing.pro}
              planId="pro"
              annual={annual}
              b={pricing.billing}
              featured
            />
          </Reveal>
        </div>
        <Bundle b={pricing.bundle} />
      </div>
    </section>
  );
}
