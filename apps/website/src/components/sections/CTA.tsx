import { Reveal } from '@/components/animations/Reveal';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function CTA({ t, refCode }: { t: Dictionary; refCode?: string }) {
  return (
    <section id="get-started" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal y={24}>
          <div className="relative overflow-hidden rounded-[32px] border border-border/80 bg-[linear-gradient(145deg,rgba(216,246,255,0.92),rgba(247,253,255,0.88)_48%,rgba(255,255,255,0.96))] px-6 py-[clamp(40px,6vw,72px)] text-center shadow-[0_28px_90px_rgba(9,11,12,0.08)] dark:bg-[linear-gradient(145deg,rgba(16,50,69,0.7),rgba(16,21,26,0.96)_52%,rgba(20,24,27,0.92))]">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 bg-[radial-gradient(80%_120%_at_50%_-30%,rgba(151,229,255,0.6),transparent_60%),radial-gradient(60%_80%_at_100%_100%,rgba(0,166,244,0.12),transparent_58%)]"
            />
            <div className="relative mx-auto max-w-[40ch]">
              <h2 className="font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
                {t.cta.title}
              </h2>
              <p className="mt-3.5 text-[17px] text-muted">{t.cta.sub}</p>
              <WaitlistForm refCode={refCode} labels={t.waitlist} />
              <p className="mt-3.5 text-xs text-muted">{t.cta.note}</p>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
