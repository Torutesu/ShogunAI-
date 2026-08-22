import { ArrowDown, ArrowRight, Check, LockKeyhole, Search } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { BrandIcon } from '@/components/BrandIcon';
import { AnimatedLogo } from '@/components/AnimatedLogo';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

const MEMORY_SOURCES = [
  { domain: 'gmail.com', name: 'Gmail', label: 'Gmail' },
  { domain: 'notion.so', name: 'Notion', label: 'Notion' },
  { domain: 'calendar.google.com', name: 'Google Calendar', label: 'Calendar' },
] as const;

function MemoryPoints({ items }: { items: readonly string[] }) {
  return (
    <ul className="mt-6 grid gap-2 sm:grid-cols-3 lg:grid-cols-1 xl:grid-cols-3">
      {items.map((item) => (
        <li key={item} className="flex items-center gap-2 rounded-full border border-border bg-surface px-3 py-2 text-[12px] font-medium text-ink">
          <span className="flex size-4 shrink-0 items-center justify-center rounded-full bg-sky-soft">
            <Check className="size-2.5 text-accent" strokeWidth={3} />
          </span>
          <span>{item}</span>
        </li>
      ))}
    </ul>
  );
}

function FlowConnector() {
  return (
    <div className="flex h-7 items-center justify-center" aria-hidden="true">
      <span className="flex size-5 items-center justify-center rounded-full border border-border bg-surface text-accent shadow-sm">
        <ArrowDown className="size-3" strokeWidth={2.5} />
      </span>
    </div>
  );
}

export function Memory({ t, locale }: { t: Dictionary; locale: Locale }) {
  const m = t.memory;

  return (
    <section id="memory" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-12 lg:grid-cols-2 lg:gap-16">
        <Reveal>
          <p className="text-xs font-semibold tracking-[0.04em] text-accent">{m.eyebrow}</p>
          <h2 className="memory-title mt-4 font-display text-[clamp(28px,5.5vw,44px)] font-semibold leading-[1.08] tracking-[-0.02em] text-balance">
            {m.title}
          </h2>
          <p className="memory-body mt-5 max-w-[640px] text-[17px] leading-relaxed text-muted">{m.body}</p>
          <MemoryPoints items={m.points} />
          <a
            href={`/${locale}/features/ai-memory`}
            className="group mt-7 inline-flex items-center gap-1.5 text-[15px] font-medium text-accent hover:text-accent-strong"
          >
            {m.cta} <ArrowRight className="size-4 transition-transform duration-300 group-hover:translate-x-1" />
          </a>
        </Reveal>

        <Reveal delay={0.1} y={24}>
          <div data-testid="memory-flow" className="overflow-hidden rounded-[28px] border border-border bg-surface shadow-[var(--shadow-card)]">
            <div className="flex items-center justify-between border-b border-border px-4 py-3.5 sm:px-5">
              <div className="flex min-w-0 items-center gap-2.5">
                <span data-mark-hover className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-sky-soft">
                  <AnimatedLogo size={19} />
                </span>
                <p className="truncate text-[13px] font-semibold text-ink">{m.uiTitle}</p>
              </div>
              <div className="ml-3 flex shrink-0 items-center gap-1.5 rounded-full border border-border bg-cloud px-2.5 py-1.5 text-[10px] font-medium text-muted">
                <LockKeyhole className="size-3 text-accent" />
                {m.uiPrivate}
              </div>
            </div>

            <div className="bg-cloud/45 p-3 sm:p-4">
              <div className="rounded-[18px] border border-border bg-surface p-3.5 sm:p-4">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-[10px] font-semibold text-accent">{m.captureLabel}</p>
                    <p className="mt-1 text-[13px] font-semibold text-ink">{m.captureTitle}</p>
                  </div>
                  <span className="size-2 rounded-full bg-emerald-400 shadow-[0_0_9px_rgba(52,211,153,0.55)]" />
                </div>
                <div className="mt-3 grid grid-cols-3 gap-2">
                  {MEMORY_SOURCES.map((source) => (
                    <div key={source.domain} className="flex items-center gap-2 rounded-[12px] border border-border bg-cloud px-2.5 py-2.5">
                      <span className="flex size-7 shrink-0 items-center justify-center rounded-[8px] border border-border bg-white">
                        <BrandIcon domain={source.domain} name={source.name} size={16} />
                      </span>
                      <span className="min-w-0 truncate text-[10px] font-medium text-ink">{source.label}</span>
                    </div>
                  ))}
                </div>
              </div>

              <FlowConnector />

              <div className="rounded-[18px] border border-accent/20 bg-sky-soft p-3.5 sm:p-4">
                <p className="text-[10px] font-semibold text-accent">{m.rememberLabel}</p>
                <div className="mt-2 flex items-start gap-3">
                  <span className="flex size-9 shrink-0 items-center justify-center rounded-[10px] border border-accent/15 bg-surface">
                    <BrandIcon domain="notion.so" name="Notion" size={18} />
                  </span>
                  <div className="min-w-0">
                    <p className="text-[13px] font-semibold text-ink">{m.rememberTitle}</p>
                    <p className="mt-1 text-[11px] leading-relaxed text-muted">{m.rememberBody}</p>
                    <p className="mt-2 text-[9px] font-medium text-accent">{m.rememberMeta}</p>
                  </div>
                </div>
              </div>

              <FlowConnector />

              <div className="rounded-[18px] border border-border bg-surface p-3.5 sm:p-4">
                <div className="flex items-center gap-2 text-[10px] font-semibold text-accent">
                  <Search className="size-3.5" strokeWidth={2.4} />
                  {m.recallLabel}
                </div>
                <p className="mt-2 rounded-[12px] bg-cloud px-3 py-2.5 text-[11px] font-medium text-ink">{m.recallQuestion}</p>
                <div className="mt-2 flex items-start gap-2 rounded-[12px] border border-accent/15 bg-sky-soft px-3 py-2.5">
                  <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full bg-accent text-white">
                    <Check className="size-2.5" strokeWidth={3} />
                  </span>
                  <p className="text-[11px] leading-relaxed text-ink">{m.recallAnswer}</p>
                </div>
              </div>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
