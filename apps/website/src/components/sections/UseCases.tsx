import { Reveal } from '@/components/animations/Reveal';
import { Logo } from '@/components/Logo';
import type { Dictionary } from '@/i18n/dictionaries';

type Item = Dictionary['usecases']['items'][number];

function DemoCard({ item, i }: { item: Item; i: number }) {
  return (
    <Reveal delay={i * 0.08} y={26}>
      <figure className="lift flex h-full flex-col rounded-xl border border-border bg-surface p-6 shadow-[var(--shadow-card)] hover:border-accent/40">
        <div className="mb-3 flex items-center justify-between gap-2">
          <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-accent">{item.persona}</span>
          <span className="rounded-full border border-border bg-cloud px-2.5 py-0.5 text-[11px] font-medium text-muted">
            {item.chip}
          </span>
        </div>
        <h3 className="font-display text-lg font-semibold leading-snug tracking-[-0.01em]">{item.title}</h3>
        <p className="mt-2 text-sm leading-relaxed text-muted">{item.body}</p>

        {/* Animated mini exchange */}
        <div className="mt-5 space-y-2.5 rounded-lg border border-border bg-cloud p-3.5">
          <div className="flex justify-end">
            <p className="max-w-[85%] rounded-2xl rounded-br-sm bg-ink px-3 py-2 text-[13px] leading-snug text-on-ink">
              {item.q}
            </p>
          </div>
          <div className="flex items-start gap-2">
            <span className="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-full bg-sky-soft">
              <Logo size={13} />
            </span>
            <p className="max-w-[85%] rounded-2xl rounded-tl-sm border border-border bg-surface px-3 py-2 text-[13px] leading-snug text-ink">
              {item.a}
            </p>
          </div>
        </div>
      </figure>
    </Reveal>
  );
}

export function UseCases({ t }: { t: Dictionary }) {
  return (
    <section id="usecases" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[48ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.usecases.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.usecases.title}
          </h2>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">{t.usecases.sub}</p>
        </Reveal>

        <div className="grid gap-5 md:grid-cols-3">
          {t.usecases.items.map((item, i) => (
            <DemoCard key={item.title} item={item} i={i} />
          ))}
        </div>
      </div>
    </section>
  );
}
