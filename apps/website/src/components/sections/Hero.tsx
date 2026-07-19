import { Logo } from '@/components/Logo';
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
    <section className="relative overflow-hidden pt-14 isolate">
      {/* Sky backdrop + drifting orbs */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 -z-20 bg-[radial-gradient(120%_80%_at_50%_-10%,var(--color-sky-soft)_0%,var(--color-cloud)_45%,var(--color-bg)_75%)]"
      />
      <span aria-hidden="true" className="orb -left-24 -top-28 -z-10 size-[520px] bg-[#8fe3ff]" />
      <span aria-hidden="true" className="orb -right-28 -top-10 -z-10 size-[460px] bg-[#bfeaff] [animation-delay:-6s]" />
      <span aria-hidden="true" className="orb left-[35%] top-56 -z-10 size-[420px] bg-[#d9f6ff] [animation-delay:-11s]" />

      <div className="container-x pt-10 text-center">
        <Reveal>
          {invite ? (
            <Badge dot>
              {t.hero.invitedBy(invite.inviter)}
              {invite.tier ? t.hero.invitedTier(invite.tier.label) : ''}
            </Badge>
          ) : (
            <Badge dot>{t.hero.badge}</Badge>
          )}
        </Reveal>

        <Reveal delay={0.05}>
          <h1 className="mx-auto mt-6 max-w-[16ch] font-display text-[clamp(40px,6.4vw,72px)] font-semibold leading-[1.03] tracking-[-0.03em] text-balance">
            {t.hero.lineA}
            <br />
            {t.hero.lineB}
          </h1>
        </Reveal>

        <Reveal delay={0.1}>
          <p className="mx-auto mt-5 max-w-[44ch] text-[18px] leading-relaxed text-muted">{t.hero.sub}</p>
        </Reveal>

        <Reveal delay={0.15}>
          <WaitlistForm refCode={refCode} labels={t.waitlist} />
          <p className="mt-4 text-xs text-muted">{t.hero.note}</p>
        </Reveal>

        <Reveal delay={0.18}>
          <Scarcity t={t} joined={joined} />
        </Reveal>

        <Reveal delay={0.2}>
          <Badges t={t} />
        </Reveal>

        <Reveal delay={0.25} y={28}>
          <div className="animate-floaty">
            <HeroMock t={t} />
          </div>
        </Reveal>
      </div>
    </section>
  );
}

function Scarcity({ t, joined }: { t: Dictionary; joined: number }) {
  const pct = Math.min(100, Math.round((joined / WAITLIST_GOAL) * 100));
  return (
    <div className="mx-auto mt-8 max-w-[420px]">
      <div className="mb-2.5 flex items-center justify-center gap-2">
        <span className="inline-flex items-center gap-1.5 rounded-full border border-[#bfeeff] bg-sky-soft px-2.5 py-1 text-[11px] font-semibold text-accent-strong">
          <span className="relative flex size-1.5">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-accent opacity-70" />
            <span className="relative inline-flex size-1.5 rounded-full bg-accent" />
          </span>
          {t.scarcity.limited}
        </span>
      </div>
      <div className="flex items-center justify-center gap-2.5">
        <AvatarStack />
        <p className="text-sm text-muted">
          <span className="font-display text-[15px] font-semibold tabular-nums text-ink">
            <CountUp value={joined} />+
          </span>{' '}
          {t.scarcity.joinedSuffix}
        </p>
      </div>
      <div className="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-border">
        <div
          className="h-full rounded-full bg-gradient-to-r from-accent to-accent-strong transition-[width] duration-700 ease-out"
          style={{ width: `${Math.max(pct, 3)}%` }}
        />
      </div>
      <p className="mt-2 text-[11px] text-faint">
        {pct}% {t.scarcity.goalLabel}
      </p>
    </div>
  );
}

/**
 * Social-proof avatar stack. Drop real photos / X avatars into
 * `public/avatars/` and set `src` below — items with no `src` fall back to a
 * tasteful monogram tile (never the old flat dots).
 */
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

function HeroMock({ t }: { t: Dictionary }) {
  const h = t.hero;
  return (
    <div className="mx-auto mt-14 max-w-[960px] overflow-hidden rounded-xl border border-border bg-surface text-left shadow-[0_24px_70px_rgba(9,11,12,0.12)]">
      <div className="flex h-11 items-center gap-2 border-b border-border bg-cloud px-4">
        <span className="size-2.5 rounded-full bg-[#e0e4e7]" />
        <span className="size-2.5 rounded-full bg-[#e0e4e7]" />
        <span className="size-2.5 rounded-full bg-[#e0e4e7]" />
        <div className="ml-3 flex h-[26px] max-w-[320px] flex-1 items-center gap-2 rounded-full border border-border bg-surface px-3 text-xs text-muted">
          <span className="text-accent">◈</span> app.shogunai.com
        </div>
      </div>
      <div className="grid min-h-[320px] grid-cols-1 sm:grid-cols-[190px_1fr]">
        <aside className="hidden flex-col gap-1 border-r border-border bg-cloud p-4 sm:flex">
          <div className="mb-3 flex items-center gap-2 font-display text-sm font-semibold">
            <Logo size={18} /> ShogunAI
          </div>
          <div className="rounded-lg bg-surface px-2.5 py-2 text-[13px] font-medium text-ink shadow-[var(--shadow-card)]">
            {h.mockNav[0]}
          </div>
          {h.mockNav.slice(1, 3).map((x) => (
            <div key={x} className="px-2.5 py-2 text-[13px] font-medium text-muted">
              {x}
            </div>
          ))}
        </aside>
        <div className="p-6">
          <div className="mb-4 flex items-start justify-between gap-4">
            <div>
              <div className="text-[10px] font-semibold tracking-[0.1em] text-muted">{h.mockToday}</div>
              <div className="mt-1 font-display text-xl font-semibold">{h.mockHeading}</div>
            </div>
            <Badge dot>{h.mockLive}</Badge>
          </div>
          <div className="mb-4 grid grid-cols-1 gap-3 sm:grid-cols-3">
            <Tile k={h.mockCaptured} n={2481} s={h.mockCapturedSub} />
            <Tile k={h.mockRecalled} n={14} s={h.mockRecalledSub} />
            <Tile k={h.mockActed} n={9} s={h.mockActedSub} accent />
          </div>
          {[h.mockRow1, h.mockRow2, h.mockRow3].map((row, i) => (
            <div
              key={row}
              className="mb-2 flex items-center justify-between rounded-lg border border-border bg-surface px-3.5 py-3"
            >
              <span className="text-[13px] font-medium text-ink">
                <span className="mr-2 font-bold text-accent">✓</span>
                {row}
              </span>
              <span className="text-[11px] text-faint">{['2m', '18m', '1h'][i]}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Tile({ k, n, s, accent }: { k: string; n: number; s: string; accent?: boolean }) {
  return (
    <div className={`rounded-lg border p-3.5 ${accent ? 'border-[#bfeeff] bg-sky-soft' : 'border-border bg-surface'}`}>
      <div className="text-[11px] font-semibold uppercase tracking-[0.04em] text-muted">{k}</div>
      <CountUp value={n} className="mt-1.5 block font-display text-[26px] font-semibold tabular-nums" />
      <div className={`mt-0.5 text-xs ${accent ? 'text-accent-strong' : 'text-muted'}`}>{s}</div>
    </div>
  );
}
