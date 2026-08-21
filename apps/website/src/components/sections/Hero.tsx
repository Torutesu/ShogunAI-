import { Badges } from '@/components/sections/Badges';
import { HeroVideo } from '@/components/HeroVideo';
import { Badge } from '@/components/ui/badge';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function Hero({ t }: { t: Dictionary; participantCount: number }) {
  const localeCopy = t.nav.langLabel === '言語'
    ? { waitlistProof: '熱狂中', preview: 'ライブ製品', people: 'アーリーアクセス参加者' }
    : t.nav.langLabel === 'Idioma'
      ? { waitlistProof: 'En plena ebullición', preview: 'PRODUCTO EN VIVO', people: 'Participantes del acceso anticipado' }
      : t.nav.langLabel === 'Sprache'
        ? { waitlistProof: 'Gerade in vollem Gange', preview: 'LIVE-PRODUKT', people: 'Teilnehmende am Early Access' }
        : { waitlistProof: 'Buzzing right now', preview: 'LIVE PRODUCT', people: 'Early-access participants' };
  return (
    <section className="hero-shell relative isolate overflow-hidden">
      <div aria-hidden="true" className="hero-overlay absolute inset-0 -z-20" />
      <div aria-hidden="true" className="hero-grid pointer-events-none absolute inset-0 -z-10" />

      <div className="hero-layout container-x py-6 sm:py-7 lg:pb-4 lg:pt-6">
        <div className="grid min-h-0 items-center gap-x-9 gap-y-3 lg:grid-cols-[minmax(360px,0.72fr)_minmax(0,1.28fr)] lg:gap-y-0">
          <div className="order-1 min-w-0 max-w-[520px] text-center lg:text-left">
          <div>
            <div>
              <Badge dot>{t.hero.badge}</Badge>
            </div>

            <div>
              <h1 className="hero-title mt-5 max-w-[15ch] font-display text-[clamp(36px,4.5vw,58px)] font-semibold leading-[0.98] tracking-[-0.055em] text-balance sm:mt-6">
                {t.hero.lineA}
                <br />
                <span className="hero-accent">{t.hero.lineB}</span>
              </h1>
            </div>

            <div>
              <p className="hero-subtitle mx-auto mt-5 max-w-[32rem] text-[15px] leading-[1.65] text-[#263653] sm:text-[16px] lg:mx-0">
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
              </div>
              <div className="mx-auto mt-6 max-w-[520px] lg:mx-0">
                <WaitlistForm labels={t.waitlist} />
              </div>
            </div>
          </div>
          </div>

          <div className="hero-video-frame order-4 w-full min-w-0 max-w-[1320px] lg:order-2 lg:justify-self-end">
            <HeroVideo label={`${localeCopy.preview} — ShogunAI for macOS`} />
          </div>

          <div className="hero-badges order-2 mt-5 lg:order-3 lg:col-span-2 lg:mt-9">
            <Badges t={t} />
          </div>

        </div>
      </div>
    </section>
  );
}
