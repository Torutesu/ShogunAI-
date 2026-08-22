import {
  ArrowRight,
  Check,
  FileText,
  LockKeyhole,
  Mail,
  Paperclip,
  ShieldCheck,
} from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { BrandIcon } from '@/components/BrandIcon';
import { AnimatedLogo } from '@/components/AnimatedLogo';
import { Button } from '@/components/ui/button';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

const SOURCE_BRANDS = [
  { domain: 'gmail.com', name: 'Gmail' },
  { domain: 'notion.so', name: 'Notion' },
  { domain: 'calendar.google.com', name: 'Google Calendar' },
] as const;

export function Action({ t, locale }: { t: Dictionary; locale: Locale }) {
  const a = t.action;
  const explore = { en: 'Explore execution layer', ja: '実行レイヤーを見る', es: 'Explorar la capa de ejecución', de: 'Ausführungsebene ansehen' }[locale];

  return (
    <section id="action" className="theme-soft-section scroll-mt-20 bg-cloud py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid items-center gap-12 lg:grid-cols-2 lg:gap-16">
        <Reveal delay={0.1} y={24} className="order-2 lg:order-1">
          <div data-testid="execution-console" className="overflow-hidden rounded-[28px] border border-border bg-surface shadow-[var(--shadow-card)]">
            <div className="flex items-center justify-between border-b border-border px-4 py-3.5 sm:px-5">
              <div className="flex min-w-0 items-center gap-2.5">
                <span data-mark-hover className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-sky-soft">
                  <AnimatedLogo size={19} />
                </span>
                <div className="min-w-0">
                  <p className="truncate text-[13px] font-semibold text-ink">ShogunAI</p>
                  <p className="truncate text-[10px] text-muted">{a.uiWorkspace}</p>
                </div>
              </div>
              <div className="ml-3 flex shrink-0 items-center gap-1.5 rounded-full border border-border bg-cloud px-2.5 py-1.5 text-[10px] font-medium text-muted">
                <span className="size-1.5 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.65)]" />
                {a.uiStatus}
              </div>
            </div>

            <div className="grid gap-3 p-3 sm:p-4 md:grid-cols-[0.82fr_1.18fr]">
              <div className="rounded-[20px] border border-border bg-cloud p-3.5 sm:p-4">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-accent">{a.uiContextEyebrow}</p>
                    <p className="mt-1 text-[13px] font-semibold text-ink">{a.uiContextTitle}</p>
                  </div>
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-sky-soft text-accent">
                    <ShieldCheck className="size-4" strokeWidth={2.2} />
                  </span>
                </div>

                <div className="mt-4 grid gap-2.5">
                  {a.uiSources.map((source, index) => {
                    const brand = SOURCE_BRANDS[index];
                    return (
                      <div
                        key={source.label}
                        className="group flex items-center gap-2.5 rounded-[14px] border border-border bg-surface px-2.5 py-2.5 transition-colors hover:border-accent/35 hover:bg-sky-soft/40"
                      >
                        <span className="flex size-8 shrink-0 items-center justify-center rounded-[9px] border border-border bg-white">
                          <BrandIcon domain={brand.domain} name={brand.name} size={18} />
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-[11px] font-semibold text-ink">{source.label}</p>
                          <p className="mt-0.5 truncate text-[9px] text-muted">{source.meta}</p>
                        </div>
                        <Check className="size-3.5 shrink-0 text-accent" strokeWidth={2.5} />
                      </div>
                    );
                  })}
                </div>

                <div className="mt-3 flex items-center gap-2 rounded-[12px] border border-accent/15 bg-sky-soft px-3 py-2 text-[10px] font-medium text-accent">
                  <FileText className="size-3.5 shrink-0" />
                  <span>{a.uiContextReady}</span>
                </div>
              </div>

              <div className="overflow-hidden rounded-[20px] border border-border bg-surface">
                <div className="flex items-center justify-between border-b border-border bg-cloud/70 px-4 py-3">
                  <div className="flex items-center gap-2">
                    <span className="flex size-7 items-center justify-center rounded-[8px] bg-sky-soft text-accent">
                      <Mail className="size-3.5" />
                    </span>
                    <div>
                      <p className="text-[10px] font-semibold text-ink">{a.uiDraftTitle}</p>
                      <p className="text-[9px] text-muted">{a.uiDraftApp}</p>
                    </div>
                  </div>
                  <span className="rounded-full bg-sky-soft px-2 py-1 text-[9px] font-semibold text-accent">{a.uiDraftState}</span>
                </div>

                <div className="px-4 pt-3">
                  <div className="grid grid-cols-[52px_1fr] items-center border-b border-border py-2 text-[10px]">
                    <span className="text-muted">{a.uiToLabel}</span>
                    <div className="flex items-center gap-2 font-medium text-ink">
                      <span className="flex size-5 items-center justify-center rounded-full bg-[#e8eeff] text-[8px] font-bold text-[#004cfc]">M</span>
                      {a.uiRecipient}
                    </div>
                  </div>
                  <div className="grid grid-cols-[52px_1fr] items-start border-b border-border py-2 text-[10px]">
                    <span className="pt-0.5 text-muted">{a.uiSubjectLabel}</span>
                    <span className="font-medium leading-relaxed text-ink">{a.uiSubject}</span>
                  </div>

                  <p className="min-h-[72px] py-3 text-[10px] leading-[1.6] text-muted">{a.uiDraftBody}</p>

                  <div className="flex items-center gap-2 rounded-[11px] border border-border bg-cloud px-2.5 py-2">
                    <Paperclip className="size-3.5 shrink-0 text-accent" />
                    <span className="min-w-0 flex-1 truncate text-[9px] font-medium text-ink">{a.uiAttachment}</span>
                    <Check className="size-3 shrink-0 text-accent" strokeWidth={2.5} />
                  </div>
                </div>

                <div className="mt-3 flex items-center justify-between gap-2 border-t border-border bg-cloud/60 px-3 py-3 sm:px-4">
                  <div className="flex min-w-0 items-center gap-1.5 text-[9px] font-medium text-muted">
                    <LockKeyhole className="size-3.5 shrink-0 text-accent" />
                    <span className="truncate">{a.uiApproval}</span>
                  </div>
                  <a
                    href="#get-started"
                    className="shrink-0 rounded-full bg-[linear-gradient(100deg,var(--cta-start),var(--cta-end))] px-3 py-2 text-[9px] font-semibold text-[var(--cta-ink)] shadow-sm transition-transform hover:-translate-y-0.5"
                  >
                    {a.uiReview}
                  </a>
                </div>
              </div>
            </div>

            <div className="grid grid-cols-3 border-t border-border bg-cloud/55 px-3 py-2.5 sm:px-4">
              {a.uiTrace.map((label, index) => (
                <div key={label} className="flex min-w-0 items-center justify-center gap-1.5 px-1 text-[9px] font-medium text-muted">
                  <span className={`flex size-4 shrink-0 items-center justify-center rounded-full ${index === 2 ? 'bg-sky-soft text-accent' : 'bg-accent text-white'}`}>
                    {index === 2 ? <LockKeyhole className="size-2.5" /> : <Check className="size-2.5" strokeWidth={3} />}
                  </span>
                  <span className="truncate">{label}</span>
                </div>
              ))}
            </div>
          </div>
        </Reveal>

        <Reveal className="order-1 lg:order-2">
          <p className="text-xs font-semibold tracking-[0.04em] text-accent">{a.eyebrow}</p>
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
