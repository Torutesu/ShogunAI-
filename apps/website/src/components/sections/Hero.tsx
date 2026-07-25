import { Reveal } from '@/components/animations/Reveal';
import { CountUp } from '@/components/CountUp';
import { Badges } from '@/components/sections/Badges';
import { Badge } from '@/components/ui/badge';
import { WaitlistForm } from '@/components/WaitlistForm';
import type { Dictionary } from '@/i18n/dictionaries';

type Invite = { inviter: string; tier: { label: string } | null } | null;

const WAITLIST_GOAL = 10000;

export function Hero({
  t,
  refCode,
  invite,
  joined = 468,
}: {
  t: Dictionary;
  refCode?: string;
  invite: Invite;
  joined?: number;
}) {
  return (
    <section className="hero-shell relative overflow-hidden pt-8 isolate sm:pt-14">
      {/* Sky backdrop + soft cloud field */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 -z-20 bg-[linear-gradient(180deg,#86ddfb_0%,#a8e9fc_38%,#c5f2ff_72%,#eefcff_100%)]"
      />
      <div aria-hidden="true" className="hero-haze pointer-events-none absolute inset-0 -z-10" />
      <div aria-hidden="true" className="hero-cloud hero-cloud--left" />
      <div aria-hidden="true" className="hero-cloud hero-cloud--right" />
      <div aria-hidden="true" className="hero-cloud hero-cloud--bottom" />
      <div aria-hidden="true" className="hero-cloud hero-cloud--far" />
      <div aria-hidden="true" className="hero-grid pointer-events-none absolute inset-x-0 top-0 -z-10 h-[720px]" />

      <div className="container-x pt-10 pb-14 lg:pt-16 lg:pb-18">
        <div className="mx-auto flex max-w-[880px] flex-col items-center text-center">
          <Reveal>
            <div className="flex flex-wrap items-center justify-center gap-2.5">
              {invite ? (
                <Badge dot>
                  {t.hero.invitedBy(invite.inviter)}
                  {invite.tier ? t.hero.invitedTier(invite.tier.label) : ''}
                </Badge>
              ) : (
                <Badge dot>{t.hero.badge}</Badge>
              )}
              <span className="inline-flex items-center gap-2 rounded-full border border-black/8 bg-white/28 px-3 py-1.5 text-xs font-medium text-[#0b5062] shadow-[0_10px_30px_rgba(0,76,110,0.08)] backdrop-blur-md">
                <span className="h-2 w-2 rounded-full bg-accent shadow-[0_0_0_4px_rgba(0,166,244,0.12)]" />
                <span className="font-semibold text-ink">
                  <CountUp value={joined} />+
                </span>
                {t.scarcity.joinedSuffix}
              </span>
            </div>
          </Reveal>

          <Reveal delay={0.05}>
            <h1 className="mx-auto mt-7 max-w-[14ch] font-display text-[clamp(38px,7vw,84px)] font-semibold leading-[0.94] tracking-[-0.05em] text-balance">
              {t.hero.lineA}
              <br />
              <span className="hero-accent">{t.hero.lineB}</span>
            </h1>
          </Reveal>

          <Reveal delay={0.1}>
            <p className="mx-auto mt-5 max-w-[42ch] text-[18px] leading-relaxed text-[#164b5d] lg:text-[19px]">
              {t.hero.sub}
            </p>
          </Reveal>

          <Reveal delay={0.15}>
            <div className="mx-auto mt-2 w-full max-w-[640px]">
              <WaitlistForm refCode={refCode} labels={t.waitlist} />
            </div>
            <p className="mt-4 text-xs text-[#2b6173]/80">{t.hero.note}</p>
          </Reveal>

          <Reveal delay={0.2}>
            <div className="mt-10 flex w-full flex-col items-center gap-5">
              <Scarcity t={t} joined={joined} />
              <Badges t={t} />
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}

function Scarcity({ t, joined }: { t: Dictionary; joined: number }) {
  const pct = Math.min(100, Math.round((joined / WAITLIST_GOAL) * 100));
  return (
    <div className="w-full max-w-[460px] rounded-[28px] border border-white/45 bg-white/24 p-4 text-left shadow-[0_24px_80px_rgba(0,68,104,0.12)] backdrop-blur-md">
      <div className="mb-3 flex items-center justify-between gap-3">
        <span className="inline-flex items-center gap-1.5 rounded-full border border-white/60 bg-white/42 px-2.5 py-1 text-[11px] font-semibold text-[#08556d]">
          <span className="relative flex size-1.5">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-accent opacity-70" />
            <span className="relative inline-flex size-1.5 rounded-full bg-accent" />
          </span>
          {t.scarcity.limited}
        </span>
        <p className="text-[11px] font-medium text-[#236377]">
          {pct}% {t.scarcity.goalLabel}
        </p>
      </div>
      <div className="flex items-center gap-3">
        <AvatarStack />
        <p className="text-sm text-[#1a5a6d]">
          People already moving up the queue with referrals.
        </p>
      </div>
      <div className="mt-3.5 h-2 w-full overflow-hidden rounded-full bg-white/35">
        <div
          className="h-full rounded-full bg-gradient-to-r from-accent via-[#36c6ff] to-accent-strong transition-[width] duration-700 ease-out"
          style={{ width: `${Math.max(pct, 3)}%` }}
        />
      </div>
    </div>
  );
}

type Face = { name: string; src?: string; tint: string };
const FACES: Face[] = [
  { name: 'Mika', tint: 'linear-gradient(135deg,#00a6f4,#0089cf)' },
  { name: 'Devin', tint: 'linear-gradient(135deg,#f0b232,#e08a00)' },
  { name: 'Alex', tint: 'linear-gradient(135deg,#eb459e,#b12e73)' },
  { name: 'Kenji', tint: 'linear-gradient(135deg,#23a55a,#158043)' },
  { name: 'Sara', tint: 'linear-gradient(135deg,#5865F2,#3b45c0)' },
];

function AvatarStack() {
  return (
    <div className="flex -space-x-2.5">
      {FACES.map((f) => (
        <span
          key={f.name}
          title={f.name}
          className="grid size-8 place-items-center overflow-hidden rounded-full border-2 border-bg text-[11px] font-semibold text-white shadow-[0_1px_3px_rgba(9,11,12,0.18)] ring-1 ring-black/5"
          style={f.src ? undefined : { backgroundImage: f.tint }}
        >
          {f.src ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={f.src} alt={f.name} className="size-full object-cover" />
          ) : (
            f.name.charAt(0)
          )}
        </span>
      ))}
    </div>
  );
}
