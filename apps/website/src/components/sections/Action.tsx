import { ArrowRight, Check } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { Button } from '@/components/ui/button';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

export function Action({ t, locale }: { t: Dictionary; locale: Locale }) {
  const a = t.action;
  const explore = { en: 'Explore execution layer', ja: '実行レイヤーを見る', es: 'Explorar la capa de ejecución', de: 'Ausführungsebene ansehen' }[locale];
  return (
    <section id="action" className="theme-soft-section scroll-mt-20 bg-cloud py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-16 md:grid-cols-2">
        {/* One request, two outcomes, side by side — the difference has to be
          * readable without reading the copy beside it. */}
        <Reveal delay={0.1} y={24} className="order-2 md:order-1">
          <div className="rounded-[26px] border border-border bg-surface p-4 shadow-[var(--shadow-card)] sm:p-5">
            <p className="rounded-[16px] bg-cloud px-4 py-3 text-center text-[13px] font-medium leading-relaxed text-ink">
              {a.comparePrompt}
            </p>
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <div className="flex flex-col rounded-[18px] border border-border bg-[linear-gradient(180deg,rgba(255,255,255,0.98),rgba(247,249,253,0.98))] p-4">
                <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-muted">{a.stepNoticeK}</span>
                <p className="mt-2 text-[13px] leading-relaxed text-muted">{a.stepNoticeV}</p>
                <div className="mt-auto pt-4">
                  <div className="h-px w-full bg-border" />
                  <span className="mt-3 block text-[10px] font-semibold uppercase tracking-[0.08em] text-muted">{a.compareLeftFootK}</span>
                  <p className="mt-1 text-[12px] leading-relaxed text-muted">{a.compareLeftFootV}</p>
                </div>
              </div>
              <div className="flex flex-col rounded-[18px] border border-[#9bb4ff] bg-[linear-gradient(180deg,#f5f7ff_0%,#e8eeff_100%)] p-4 shadow-[0_18px_44px_rgba(0,76,252,0.12)]">
                <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-accent">{a.stepActK}</span>
                <p className="mt-2 text-[13px] font-medium leading-relaxed text-ink">{a.stepActV}</p>
                <div className="mt-auto pt-4">
                  <div className="h-px w-full bg-[#9bb4ff]/50" />
                  <span className="mt-3 block text-[10px] font-semibold uppercase tracking-[0.08em] text-accent">{a.stepConfirmK}</span>
                  <p className="mt-1 text-[12px] font-medium leading-relaxed text-ink">{a.stepConfirmV}</p>
                </div>
              </div>
            </div>
          </div>
        </Reveal>

        <Reveal className="order-1 md:order-2">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{a.eyebrow}</p>
          <h2 className="action-title mt-4 font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {a.title}
          </h2>
          <p className="action-body mt-5 text-[17px] leading-relaxed text-muted">{a.body}</p>
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
          <div className="flex flex-wrap items-center gap-4">
            <Button asChild><a href="#get-started">{a.cta}</a></Button>
            <a href={`/${locale}/features/execution-layer`} className="inline-flex items-center gap-1.5 text-sm font-semibold text-accent hover:text-accent-strong">{explore} <ArrowRight className="size-4" /></a>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
