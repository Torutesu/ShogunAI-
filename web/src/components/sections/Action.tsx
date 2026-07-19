import { ArrowDown, Check } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

export function Action({ t }: { t: Dictionary }) {
  const a = t.action;
  const steps = [
    { k: a.stepNoticeK, v: a.stepNoticeV, accent: false },
    { k: a.stepActK, v: a.stepActV, accent: true },
    { k: a.stepConfirmK, v: a.stepConfirmV, accent: false },
  ];
  return (
    <section id="action" className="scroll-mt-20 bg-cloud py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-16 md:grid-cols-2">
        <Reveal delay={0.1} y={24} className="order-2 md:order-1">
          <div className="grid gap-2">
            {steps.map((s, i) => (
              <div key={s.k}>
                <Card className={s.accent ? 'border-[#bfeeff] bg-sky-soft' : ''}>
                  <div className="text-[11px] font-semibold uppercase tracking-[0.06em] text-muted">{s.k}</div>
                  <div className="mt-1.5 text-[15px] font-medium text-ink">{s.v}</div>
                </Card>
                {i < steps.length - 1 && <ArrowDown className="mx-auto my-1 size-[18px] text-accent" />}
              </div>
            ))}
          </div>
        </Reveal>

        <Reveal className="order-1 md:order-2">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{a.eyebrow}</p>
          <h2 className="mt-4 font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {a.title}
          </h2>
          <p className="mt-5 text-[17px] leading-relaxed text-muted">{a.body}</p>
          <ul className="my-6 grid gap-3">
            {a.points.map((it) => (
              <li key={it} className="flex items-start gap-3 text-[15px] text-ink">
                <span className="mt-0.5 flex size-[18px] shrink-0 items-center justify-center rounded-full bg-sky-soft">
                  <Check className="size-3 text-accent" strokeWidth={3} />
                </span>
                {it}
              </li>
            ))}
          </ul>
          <Button asChild>
            <a href="#get-started">{a.cta}</a>
          </Button>
        </Reveal>
      </div>
    </section>
  );
}
