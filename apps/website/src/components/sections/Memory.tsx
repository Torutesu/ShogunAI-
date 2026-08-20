import { ArrowRight, Check } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

function Ticks({ items }: { items: readonly string[] }) {
  return (
    <ul className="my-6 grid gap-3">
      {items.map((it) => (
        <li key={it} className="flex items-start gap-3 text-[15px] text-ink">
          <span className="mt-0.5 flex size-[18px] shrink-0 items-center justify-center rounded-full bg-sky-soft">
            <Check className="size-3 text-accent" strokeWidth={3} />
          </span>
          {it}
        </li>
      ))}
    </ul>
  );
}

export function Memory({ t, locale }: { t: Dictionary; locale: Locale }) {
  const m = t.memory;
  return (
    <section id="memory" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-16 md:grid-cols-2">
        <Reveal>
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{m.eyebrow}</p>
          <h2 className="memory-title mt-4 font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {m.title}
          </h2>
          <p className="memory-body mt-5 text-[17px] leading-relaxed text-muted">{m.body}</p>
          <Ticks items={m.points} />
          <a href={`/${locale}/features/ai-memory`} className="group inline-flex items-center gap-1.5 text-[15px] font-medium text-accent hover:text-accent-strong">
            {m.cta} <ArrowRight className="size-4 transition-transform duration-300 group-hover:translate-x-1" />
          </a>
        </Reveal>

        <Reveal delay={0.1} y={24}>
          <Card className="lift p-5">
            <div className="mb-4 flex items-center justify-between">
              <Badge dot>{m.recallChip}</Badge>
              <span className="text-xs text-muted">0.2s</span>
            </div>
            <div className="rounded-lg border border-border bg-cloud px-4 py-3.5 text-[15px] font-medium text-ink">
              {m.recallQuestion}
            </div>
            <div className="mt-3 pl-1">
              <div className="border-b border-dashed border-border py-2 text-sm text-ink">
                <b className="font-mono text-[13px] font-medium text-accent">{m.recallLine1Time}</b> {m.recallLine1}
              </div>
              <div className="border-b border-dashed border-border py-2 text-sm text-ink">
                <b className="font-mono text-[13px] font-medium text-accent">{m.recallLine2Time}</b> {m.recallLine2}
              </div>
              <div className="mt-2.5 text-xs text-muted">{m.recallSrc}</div>
            </div>
          </Card>
        </Reveal>
      </div>
    </section>
  );
}
