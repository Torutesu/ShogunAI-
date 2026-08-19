import { ArrowRight, Check, Command, Play, Sparkles } from 'lucide-react';
import { ParticipantCount } from '@/components/ParticipantCount';
import { Badges } from '@/components/sections/Badges';
import { HeroMarqueeRow } from '@/components/sections/Marquee';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

export function Hero({ t, participantCount }: { t: Dictionary; participantCount: number }) {
  const localeCopy = t.nav.langLabel === '言語'
    ? { waitlistProof: '熱量の高いアーリーアダプターが、すでに参加中', preview: 'ライブ製品', people: 'アーリーアクセス参加者' }
    : t.nav.langLabel === 'Idioma'
      ? { waitlistProof: 'Early adopters entusiastas ya están en la lista de espera', preview: 'PRODUCTO EN VIVO', people: 'Participantes del acceso anticipado' }
      : t.nav.langLabel === 'Sprache'
        ? { waitlistProof: 'Engagierte Early Adopters sind bereits auf der Warteliste', preview: 'LIVE-PRODUKT', people: 'Teilnehmende am Early Access' }
        : { waitlistProof: 'Driven early adopters are already on the waitlist', preview: 'LIVE PRODUCT', people: 'Early-access participants' };
  const activity = [
    { label: t.hero.mockRow1, time: '10:42', icon: 'M' },
    { label: t.hero.mockRow2, time: '10:38', icon: 'D' },
    { label: t.hero.mockRow3, time: '10:31', icon: 'S' },
  ];

  return (
    <section className="hero-shell relative isolate overflow-hidden">
      <div aria-hidden="true" className="hero-overlay absolute inset-0 -z-20" />
      <div aria-hidden="true" className="hero-grid pointer-events-none absolute inset-0 -z-10" />

      <div className="hero-layout container-x py-8 sm:py-10 lg:pb-4 lg:pt-12">
        <div className="grid min-h-0 items-center gap-x-10 gap-y-3 lg:grid-cols-[minmax(0,0.96fr)_minmax(460px,0.82fr)] lg:gap-y-0">
          <div className="order-1 max-w-[670px] text-center lg:text-left">
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
                <span className="text-xs font-medium text-[#4b5d7d]">{localeCopy.waitlistProof}</span>
                <ParticipantCount initialCount={participantCount} suffix={t.scarcity.joinedSuffix} />
              </div>
              <div className="mx-auto mt-7 max-w-[620px] lg:mx-0">
                <WaitlistForm labels={t.waitlist} />
              </div>
            </div>
          </div>
          </div>

          <div className="hero-demo-frame order-4 w-full max-w-[1320px] lg:order-2 lg:justify-self-end">
            <div className="hero-demo-scale w-full">
              <div id="hero-demo" className="hero-demo-shell relative overflow-hidden rounded-[28px] border border-white/70 bg-[#0a1533]/90 p-3 shadow-[0_35px_90px_rgba(0,38,142,0.28)] backdrop-blur-xl sm:p-4">
              <div className="absolute inset-x-0 top-0 h-px bg-white/50" />
              <div className="flex items-center justify-between px-2 pb-3 text-[11px] font-medium text-white/62">
                <span className="flex items-center gap-2"><span className="size-2 rounded-full bg-[#7ee0af] shadow-[0_0_12px_#7ee0af]" /> {localeCopy.preview}</span>
                <span>ShogunAI for macOS</span>
              </div>

              <div className="rounded-[20px] border border-white/10 bg-[#10224d]/90 p-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.07)] sm:p-5">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="text-[11px] font-semibold tracking-[0.12em] text-[#9db2ff]">{t.hero.mockToday}</p>
                    <h2 className="mt-1.5 font-display text-[22px] font-medium tracking-[-0.035em] text-white sm:text-[25px]">{t.hero.mockHeading}</h2>
                  </div>
                  <span className="flex size-10 items-center justify-center rounded-xl bg-[#5273df]/25 text-[#b8c7ff]"><Sparkles className="size-[18px]" /></span>
                </div>

                <div className="mt-5 grid grid-cols-3 gap-2 border-y border-white/10 py-4">
                  {[
                    [t.hero.mockCaptured, '24'],
                    [t.hero.mockRecalled, '06'],
                    [t.hero.mockActed, '03'],
                  ].map(([label, value]) => (
                    <div key={label} className="min-w-0">
                      <p className="font-display text-xl font-medium text-white">{value}</p>
                      <p className="mt-1 truncate text-[10px] text-white/52">{label}</p>
                    </div>
                  ))}
                </div>

                <div className="mt-4 space-y-2">
                  {activity.map((item, index) => (
                    <div key={item.label} className="flex items-center gap-3 rounded-xl border border-white/8 bg-white/[0.055] px-3 py-2.5">
                      <span className={`flex size-7 shrink-0 items-center justify-center rounded-lg text-[11px] font-bold ${index === 0 ? 'bg-[#004cfc] text-white' : index === 1 ? 'bg-[#f0a76c] text-[#312117]' : 'bg-white text-[#18313b]'}`}>{item.icon}</span>
                      <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-white/88">{item.label}</span>
                      <span className="text-[10px] text-white/42">{item.time}</span>
                    </div>
                  ))}
                </div>

                <div className="mt-4 flex items-center gap-2 rounded-xl border border-[#7f9cff]/25 bg-[#5273df]/15 p-3 text-[12px] text-[#e4eaff]">
                  <Command className="size-4 shrink-0 text-[#aebfff]" />
                  <span className="min-w-0 flex-1 truncate">{t.hero.mockLive}</span>
                  <Check className="size-4 text-[#87e5b4]" />
                </div>
              </div>

                <Button asChild variant="secondary" size="sm" className="mt-3 w-full border-white/15 bg-white/10 text-white hover:bg-white/18 dark:bg-white/10">
                  <a href="#get-started"><Play className="size-3.5 fill-current" /> {t.nav.getStarted}<ArrowRight className="size-3.5" /></a>
                </Button>
              </div>
            </div>
          </div>

          <div className="hero-badges order-2 mt-5 lg:order-3 lg:col-span-2 lg:mt-9">
            <Badges t={t} />
          </div>

          <div className="order-3 lg:order-4 lg:col-span-2">
            <HeroMarqueeRow t={t} />
          </div>
        </div>
      </div>
    </section>
  );
}
