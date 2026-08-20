import { ArrowRight, Check, Command, Play, Sparkles } from 'lucide-react';
import { ParticipantCount } from '@/components/ParticipantCount';
import { Badges } from '@/components/sections/Badges';
import { HeroDemo } from '@/components/HeroDemo';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function Hero({ t, participantCount }: { t: Dictionary; participantCount: number }) {
  const localeCopy = t.nav.langLabel === '言語'
    ? { waitlistProof: '熱狂中', preview: 'ライブ製品', people: 'アーリーアクセス参加者' }
    : t.nav.langLabel === 'Idioma'
      ? { waitlistProof: 'En plena ebullición', preview: 'PRODUCTO EN VIVO', people: 'Participantes del acceso anticipado' }
      : t.nav.langLabel === 'Sprache'
        ? { waitlistProof: 'Gerade in vollem Gange', preview: 'LIVE-PRODUKT', people: 'Teilnehmende am Early Access' }
        : { waitlistProof: 'Buzzing right now', preview: 'LIVE PRODUCT', people: 'Early-access participants' };
  const activity = [
    { label: t.hero.mockRow1, time: '10:42', icon: 'M' },
    { label: t.hero.mockRow2, time: '10:38', icon: 'D' },
    { label: t.hero.mockRow3, time: '10:31', icon: 'S' },
  ];

  return (
    <section className="hero-shell relative isolate overflow-hidden">
      <div aria-hidden="true" className="hero-overlay absolute inset-0 -z-20" />
      <div aria-hidden="true" className="hero-grid pointer-events-none absolute inset-0 -z-10" />

      <div className="hero-layout container-x py-6 sm:py-7 lg:pb-4 lg:pt-6">
        <div className="grid min-h-0 items-center gap-x-10 gap-y-3 lg:grid-cols-[minmax(0,0.96fr)_minmax(460px,0.82fr)] lg:gap-y-0">
          <div className="order-1 min-w-0 max-w-[670px] text-center lg:text-left">
          <div>
            <div>
              <Badge dot>{t.hero.badge}</Badge>
            </div>

            <div>
              <h1 className="hero-title mt-6 max-w-[16ch] font-display text-[clamp(44px,6.5vw,78px)] font-semibold leading-[0.96] tracking-[-0.06em] text-balance sm:mt-7">
                {t.hero.lineA}
                <br />
                <span className="hero-accent">{t.hero.lineB}</span>
              </h1>
            </div>

            <div>
              <p className="hero-subtitle mx-auto mt-6 max-w-[42rem] text-[17px] leading-[1.72] text-[#263653] sm:text-[18px] lg:mx-0">
                {t.hero.sub}
              </p>
            </div>

            <div>
              <div className="hero-participants mt-5 flex flex-wrap items-center justify-center gap-3 lg:justify-start" aria-label={localeCopy.people}>
                <div className="hero-avatar-stack flex items-center">
                  {[
                    ['/optimized/queue-adobe-visitor.jpg', 'Participant'],
                    ['/optimized/queue-suited-founder.jpg', 'Participant'],
                    ['/optimized/community-monkey.png', 'Community participant'],
                    ['/optimized/community-illustration.png', 'Community participant'],
                    ['/optimized/community-owl.png', 'Community participant'],
                    ['/optimized/community-dog.png', 'Community participant'],
                  ].map(([src, alt], index) => (
                    <img
                      key={src}
                      src={src}
                      alt={alt}
                      width={36}
                      height={36}
                      className={`hero-avatar size-9 rounded-full object-cover ${index > 0 ? '-ml-2' : ''}`}
                    />
                  ))}
                  <span
                    aria-hidden="true"
                    className="-ml-2 flex size-9 shrink-0 items-center justify-center rounded-full border-2 border-white bg-white text-base font-semibold text-[#31548b] shadow-[0_4px_14px_rgba(43,108,138,0.16)]"
                  >
                    +
                  </span>
                </div>
                <span className="hero-waitlist-proof text-xs font-medium text-[#4b5d7d]">{localeCopy.waitlistProof}</span>
                <ParticipantCount initialCount={participantCount} suffix={t.scarcity.joinedSuffix} />
              </div>
              <div className="mx-auto mt-7 max-w-[620px] lg:mx-0">
                <WaitlistForm labels={t.waitlist} />
              </div>
            </div>
          </div>
          </div>

          <div className="hero-demo-frame order-4 w-full min-w-0 max-w-[1320px] lg:order-2 lg:justify-self-end">
            <div className="hero-demo-scale w-full">
              {/* The notch panel itself, running its own mock fixture outside Tauri —
                * the real surface, not a drawing of it. */}
              <HeroDemo d={t.heroDemo} cta={t.nav.getStarted} live={localeCopy.preview} macos="ShogunAI for macOS" />
            </div>
          </div>

          <div className="hero-badges order-2 mt-5 lg:order-3 lg:col-span-2 lg:mt-9">
            <Badges t={t} />
          </div>

        </div>
      </div>
    </section>
  );
}
