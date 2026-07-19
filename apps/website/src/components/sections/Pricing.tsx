'use client';

import { Check } from 'lucide-react';
import { useState } from 'react';
import { Reveal } from '@/components/animations/Reveal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

type Pricing = Dictionary['pricing'];
type PlanData = { name: string; annual: string; monthly: string; points: readonly string[]; cta: string; badge?: string };

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
    <div className="mx-auto mb-10 flex w-fit items-center gap-1 rounded-full border border-border bg-cloud p-1">
      <button
        type="button"
        onClick={() => onChange(false)}
        className={`rounded-full px-4 py-1.5 text-sm font-medium transition-colors ${
          !annual ? 'bg-surface text-ink shadow-[var(--shadow-card)]' : 'text-muted hover:text-ink'
        }`}
      >
        {b.monthly}
      </button>
      <button
        type="button"
        onClick={() => onChange(true)}
        className={`flex items-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition-colors ${
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

function Plan({ plan, annual, b, featured }: { plan: PlanData; annual: boolean; b: Pricing['billing']; featured?: boolean }) {
  const price = annual ? plan.annual : plan.monthly;
  return (
    <Card
      className={`lift relative flex flex-col p-7 ${featured ? 'border-accent shadow-[0_12px_40px_rgba(0,166,244,0.14)]' : ''}`}
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
        onClick={() => {
          // Remember which plan the visitor picked so the waitlist signup can record it.
          try {
            window.localStorage.setItem('shogun_plan_intent', `${plan.name} · ${annual ? 'annual' : 'monthly'}`);
          } catch {
            /* storage unavailable */
          }
          document.getElementById('get-started')?.scrollIntoView({ behavior: 'smooth' });
        }}
      >
        {plan.cta}
      </Button>
    </Card>
  );
}

export function Pricing({ pricing }: { pricing: Pricing }) {
  const [annual, setAnnual] = useState(true); // default: annual (recommended)
  return (
    <section id="pricing" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-8 max-w-[44ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{pricing.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {pricing.title}
          </h2>
          <p className="mt-4 text-[17px] text-muted">{pricing.sub}</p>
        </Reveal>
        <Reveal>
          <BillingToggle annual={annual} onChange={setAnnual} b={pricing.billing} />
        </Reveal>
        <div className="mx-auto grid max-w-[780px] gap-6 md:grid-cols-2">
          <Reveal>
            <Plan plan={pricing.standard} annual={annual} b={pricing.billing} />
          </Reveal>
          <Reveal delay={0.08}>
            <Plan plan={pricing.pro} annual={annual} b={pricing.billing} featured />
          </Reveal>
        </div>
      </div>
    </section>
  );
}
