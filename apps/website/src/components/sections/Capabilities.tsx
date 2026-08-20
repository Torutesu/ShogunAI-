import { CalendarCheck, Command, Languages, ListChecks, Search, Sunrise } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

/** Icons follow the item order in the dictionary; a longer list falls back to a neutral mark. */
const ICONS = [Command, CalendarCheck, Languages, Search, Sunrise, ListChecks];

export function Capabilities({ t }: { t: Dictionary }) {
  const c = t.capabilities;
  return (
    <section id="capabilities" className="scroll-mt-24 py-[clamp(48px,7vw,88px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[840px] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{c.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(28px,5vw,44px)] font-semibold leading-[1.1] tracking-[-0.02em] text-balance">
            {c.title}
          </h2>
          <p className="mx-auto mt-4 max-w-[62ch] text-[17px] leading-relaxed text-muted">{c.sub}</p>
        </Reveal>
        <div className="grid gap-5 md:grid-cols-2 lg:grid-cols-3">
          {c.items.map((item, index) => {
            const Icon = ICONS[index] ?? ListChecks;
            return (
              <Reveal key={item.name} delay={0.04 * index}>
                <Card className="lift flex h-full flex-col rounded-[26px] p-7">
                  <div className="flex items-center gap-3">
                    <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sky-soft text-accent">
                      <Icon className="size-[18px]" />
                    </span>
                    <span className="rounded-full bg-cloud px-2.5 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">
                      {item.tag}
                    </span>
                  </div>
                  <h3 className="mt-5 font-display text-[19px] font-semibold leading-[1.3] tracking-[-0.01em]">
                    {item.name}
                  </h3>
                  <p className="mt-3 text-[15px] leading-relaxed text-muted">{item.body}</p>
                </Card>
              </Reveal>
            );
          })}
        </div>
      </div>
    </section>
  );
}
