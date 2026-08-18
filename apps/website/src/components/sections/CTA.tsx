import { Reveal } from '@/components/animations/Reveal';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function CTA({ t }: { t: Dictionary }) {
  const titleLines = t.nav.langLabel === '言語'
    ? ['文脈はあちこちにある。', 'AIを毎回ゼロから始めさせない。']
    : [t.cta.title];
  return (
    <section id="get-started" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal y={24}>
          <div className="relative overflow-hidden rounded-[36px] border border-[#667fd4]/35 bg-[linear-gradient(145deg,#2746a6_0%,#142968_38%,#0a1533_72%,#070d20_100%)] px-6 py-[clamp(44px,6vw,88px)] text-center shadow-[0_36px_120px_rgba(0,38,142,0.24)]">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 bg-[radial-gradient(90%_90%_at_22%_8%,rgba(170,190,255,0.34),transparent_40%),radial-gradient(55%_55%_at_76%_18%,rgba(82,115,223,0.22),transparent_60%),linear-gradient(180deg,rgba(255,255,255,0.06),transparent_36%)]"
            />
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-0 bottom-0 h-[48%] bg-[linear-gradient(180deg,transparent_0%,rgba(7,12,20,0.14)_20%,rgba(7,12,20,0.66)_100%)]"
            />
            <div aria-hidden="true" className="pointer-events-none absolute inset-x-[4%] bottom-0 h-[44%]">
              <svg viewBox="0 0 1440 520" className="h-full w-full opacity-[0.24]" preserveAspectRatio="none">
                <defs>
                  <linearGradient id="castleGlow" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#f3d7a4" stopOpacity="0.38" />
                    <stop offset="100%" stopColor="#f3d7a4" stopOpacity="0" />
                  </linearGradient>
                </defs>
                <path
                  d="M36 520V332h118v-64h48v64h88V210h58v48h34v-92h84v92h40V146h110v72h38v-54h78v54h36V110h132v108h52v-72h92v72h44V262h60v-40h62v40h126V520Z"
                  fill="#09111a"
                />
                <rect x="522" y="174" width="16" height="30" fill="url(#castleGlow)" />
                <rect x="556" y="174" width="16" height="30" fill="url(#castleGlow)" />
                <rect x="896" y="136" width="18" height="34" fill="url(#castleGlow)" />
                <rect x="936" y="136" width="18" height="34" fill="url(#castleGlow)" />
                <rect x="1114" y="234" width="18" height="34" fill="url(#castleGlow)" />
              </svg>
            </div>
            <div className="relative mx-auto max-w-[46ch]">
              <div className="mx-auto mb-5 inline-flex items-center rounded-full border border-white/16 bg-black/14 px-4 py-1.5 text-[11px] font-semibold uppercase tracking-[0.28em] text-white/76 backdrop-blur-sm">
                ShogunAI Command Layer
              </div>
              <h2 className="cta-title font-display text-[clamp(30px,6vw,58px)] font-semibold leading-[0.98] tracking-[-0.03em] text-balance text-white">
                {titleLines.map((line) => <span key={line} className="block">{line}</span>)}
              </h2>
              <p className="cta-sub mx-auto mt-4 max-w-[34ch] text-[18px] leading-relaxed text-white/78">{t.cta.sub}</p>
              <WaitlistForm labels={t.waitlist} tone="cta" />
              <p className="mt-4 text-xs tracking-[0.02em] text-white/62">{t.cta.note}</p>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
