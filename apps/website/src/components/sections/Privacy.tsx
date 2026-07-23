import Link from 'next/link';
import { KeyRound, Lock, ShieldCheck } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

const ICONS = [Lock, ShieldCheck, KeyRound];

export function Privacy({ t }: { t: Dictionary }) {
  return (
    <section id="privacy" className="scroll-mt-20 bg-cloud py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-12 lg:grid-cols-[0.9fr_1.1fr]">
        <Reveal>
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.privacy.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(24px,5vw,42px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.privacy.title}
          </h2>
          <p className="mt-4 max-w-[46ch] text-[15px] leading-relaxed text-muted">{t.privacy.body}</p>
          <Link
            href="/privacy"
            className="group/pl mt-6 inline-flex items-center gap-1 text-sm font-semibold text-accent-strong transition-colors hover:text-accent"
          >
            {t.privacy.cta}
          </Link>
        </Reveal>

        {/* Full-width stacked cards below lg — three ~220px columns at tablet
            truncate the copy. */}
        <div className="grid gap-4">
          {t.privacy.points.map((p, i) => {
            const Icon = ICONS[i] ?? Lock;
            return (
              <Reveal key={p.title} delay={i * 0.08}>
                <div className="lift flex items-start gap-3.5 rounded-xl border border-border bg-surface p-4 shadow-[var(--shadow-card)] hover:border-accent/40">
                  <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-sky-soft text-accent-strong">
                    <Icon className="size-[18px]" strokeWidth={1.9} />
                  </span>
                  <div>
                    <div className="text-sm font-semibold text-ink">{p.title}</div>
                    <div className="mt-0.5 text-[13px] leading-relaxed text-muted">{p.body}</div>
                  </div>
                </div>
              </Reveal>
            );
          })}
        </div>
      </div>
    </section>
  );
}
