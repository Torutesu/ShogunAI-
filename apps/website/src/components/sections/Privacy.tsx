import Link from 'next/link';
import { ChevronRight, Database, Fingerprint, Lock, ShieldCheck } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

const ICONS = [Fingerprint, ShieldCheck, Database];
const TONES = [
  'from-[#8fd8ff] via-[#c9efff] to-[#f2fdff]',
  'from-[#fff1ba] via-[#fff8df] to-[#fff6c8]',
  'from-[#9acfff] via-[#d6efff] to-[#eff9ff]',
] as const;

const CHIPS = ['On-device capture', 'No cloud copy by default', 'BYOK ready'] as const;

export function Privacy({ t }: { t: Dictionary }) {
  return (
    <section id="privacy" className="scroll-mt-20 bg-[#fffdf5] py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <div className="grid items-end gap-10 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
          <Reveal>
            <div className="max-w-[24rem]">
              <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.privacy.eyebrow}</p>
              <h2 className="mt-4 font-display text-[clamp(34px,6vw,72px)] font-semibold leading-[0.96] tracking-[-0.045em] text-balance">
                {t.privacy.title}
              </h2>
              <p className="mt-5 max-w-[34ch] text-[18px] leading-relaxed text-muted">{t.privacy.body}</p>
              <Link
                href="/privacy"
                className="group/pl mt-7 inline-flex items-center gap-1 text-sm font-semibold text-accent-strong transition-colors hover:text-accent"
              >
                {t.privacy.cta}
                <ChevronRight className="size-4 transition-transform duration-200 group-hover/pl:translate-x-0.5" />
              </Link>
            </div>
          </Reveal>

          <Reveal delay={0.06}>
            <div className="overflow-hidden rounded-[34px] border border-white/60 bg-[linear-gradient(135deg,rgba(255,255,255,0.9),rgba(239,251,255,0.84))] p-5 shadow-[0_22px_70px_rgba(9,11,12,0.08)] backdrop-blur md:p-7">
              <div className="grid gap-3 sm:grid-cols-3">
                {CHIPS.map((chip) => (
                  <div key={chip} className="rounded-[22px] border border-white/75 bg-white/72 px-4 py-3 text-center text-[13px] font-semibold text-[#0d5c77] shadow-[0_10px_22px_rgba(17,40,68,0.04)]">
                    {chip}
                  </div>
                ))}
              </div>

              <div className="mt-5 grid gap-4 lg:grid-cols-[minmax(0,0.94fr)_minmax(0,1.06fr)]">
                <div className="rounded-[28px] border border-white/70 bg-[#0f1620] p-5 text-white shadow-[0_16px_42px_rgba(8,13,18,0.18)]">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-white/55">Capture policy</div>
                      <div className="mt-2 text-[28px] font-semibold tracking-[-0.03em]">You decide what is remembered.</div>
                    </div>
                    <div className="flex size-12 items-center justify-center rounded-2xl bg-white/10">
                      <Lock className="size-6" strokeWidth={1.8} />
                    </div>
                  </div>

                  <div className="mt-5 space-y-3">
                    {[
                      ['Screen memory', 'Allowed'],
                      ['Private apps', 'Ignored'],
                      ['Delete forever', 'One tap'],
                    ].map(([label, value]) => (
                      <div key={label} className="flex items-center justify-between rounded-[18px] border border-white/10 bg-white/6 px-4 py-3">
                        <span className="text-[13px] text-white/68">{label}</span>
                        <span className="rounded-full bg-white/10 px-2.5 py-1 text-[12px] font-semibold text-white">{value}</span>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="rounded-[28px] border border-[#dceef8] bg-white/80 p-5 shadow-[0_16px_42px_rgba(17,40,68,0.05)]">
                  <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-[#6d8793]">Security defaults</div>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                    {['Encrypted local store', 'No training on your data', 'Scoped sharing only', 'BYOK supported'].map((item) => (
                      <div key={item} className="rounded-[18px] border border-[#e7f2f8] bg-[#f8fdff] px-4 py-3 text-[13px] font-medium text-[#294c59]">
                        {item}
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </Reveal>
        </div>

        <div className="mt-8 grid gap-6 lg:grid-cols-3">
          {t.privacy.points.map((p, i) => {
            const Icon = ICONS[i] ?? Lock;
            return (
              <Reveal key={p.title} delay={i * 0.08 + 0.08}>
                <div className="lift overflow-hidden rounded-[30px] border border-border/70 bg-white/84 p-4 shadow-[0_18px_50px_rgba(9,11,12,0.06)]">
                  <div className={`relative mb-6 h-[240px] overflow-hidden rounded-[24px] bg-gradient-to-br ${TONES[i] ?? TONES[0]}`}>
                    <div className="absolute inset-0 bg-[radial-gradient(circle_at_18%_18%,rgba(255,255,255,0.84),transparent_18%),radial-gradient(circle_at_80%_84%,rgba(255,255,255,0.34),transparent_28%)]" />
                    <div className="absolute left-6 top-6 flex size-16 items-center justify-center rounded-[22px] bg-white/52 text-[#185f7a] shadow-[inset_0_1px_0_rgba(255,255,255,0.65)] backdrop-blur">
                      <Icon className="size-8" strokeWidth={1.8} />
                    </div>

                    {i === 0 ? (
                      <div className="absolute bottom-5 left-5 right-5 rounded-[24px] border border-white/65 bg-white/48 p-4 backdrop-blur">
                        <div className="text-sm font-semibold text-[#0f495d]">Captured locally</div>
                        <div className="mt-1 text-[13px] text-[#24586a]">Nothing leaves your machine unless you explicitly allow it.</div>
                      </div>
                    ) : i === 1 ? (
                      <div className="absolute inset-x-6 top-16 grid grid-cols-2 gap-3">
                        {['Private by default', 'No silent sync', 'Encrypted', 'Auditable'].map((chip) => (
                          <div key={chip} className="rounded-full border border-[#2b2b2b]/12 bg-white/62 px-3 py-2 text-center text-[12px] font-semibold text-[#3e3a2e] backdrop-blur">
                            {chip}
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="absolute bottom-6 right-6 rounded-[26px] border border-white/60 bg-white/42 px-5 py-4 text-right backdrop-blur">
                        <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-[#5b7684]">Model access</div>
                        <div className="mt-2 text-[28px] font-semibold tracking-[-0.03em] text-[#204252]">BYOK</div>
                        <div className="mt-1 text-[13px] text-[#5f7986]">Your provider. Your limits.</div>
                      </div>
                    )}
                  </div>

                  <div className="px-2 pb-2 text-center">
                    <div className="font-display text-[clamp(28px,3vw,42px)] font-semibold leading-[1] tracking-[-0.03em] text-ink">
                      {p.title}
                    </div>
                    <div className="mx-auto mt-4 max-w-[24ch] text-[16px] leading-relaxed text-muted">{p.body}</div>
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
