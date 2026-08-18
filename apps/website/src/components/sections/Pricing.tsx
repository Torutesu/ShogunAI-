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

function Plan({ plan, annual, b, featured }: { plan: PlanData; annual: boolean; b: Pricing['billing']; featured?: boolean }) {
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
        onClick={() => {
          document.getElementById('get-started')?.scrollIntoView({ behavior: 'smooth' });
        }}
      >
        {plan.cta}
      </Button>
    </Card>
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
