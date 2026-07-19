import { Check } from 'lucide-react';
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

function Plan({ plan, featured }: { plan: PlanData; featured?: boolean }) {
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
        <span className="font-display text-[40px] font-semibold tracking-[-0.02em]">{plan.price}</span>
        <span className="text-muted">{plan.per}</span>
      </div>
      <p className="text-sm text-muted">{plan.desc}</p>
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
      <Button asChild variant={featured ? 'primary' : 'secondary'} className="mt-auto w-full">
        <a href="#get-started">{plan.cta}</a>
      </Button>
    </Card>
  );
}

export function Pricing({ t }: { t: Dictionary }) {
  return (
    <section id="pricing" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[44ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.pricing.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.pricing.title}
          </h2>
          <p className="mt-4 text-[17px] text-muted">{t.pricing.sub}</p>
        </Reveal>
        <div className="mx-auto grid max-w-[780px] gap-6 md:grid-cols-2">
          <Reveal>
            <Plan plan={t.pricing.standard} />
          </Reveal>
          <Reveal delay={0.08}>
            <Plan plan={t.pricing.pro} featured />
          </Reveal>
        </div>
      </div>
    </section>
  );
}
