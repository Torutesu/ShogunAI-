import { Reveal } from '@/components/animations/Reveal';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function CTA({ t, refCode }: { t: Dictionary; refCode?: string }) {
  return (
    <section id="get-started" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal y={24}>
          <div className="relative overflow-hidden rounded-2xl border border-border bg-gradient-to-b from-sky-soft to-cloud px-6 py-[clamp(40px,6vw,72px)] text-center">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 bg-[radial-gradient(80%_120%_at_50%_-30%,rgba(151,229,255,0.6),transparent_60%)]"
            />
            <div className="relative mx-auto max-w-[40ch]">
              <h2 className="font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
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
