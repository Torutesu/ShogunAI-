import { Reveal } from '@/components/animations/Reveal';
import { CountUp } from '@/components/CountUp';
import type { Dictionary } from '@/i18n/dictionaries';

/** "4h" / "20+" / "100%" → count the numeric part up on scroll-in. */
function StatValue({ v }: { v: string }) {
  const m = v.match(/^(\d+)(.*)$/);
  if (!m) return <>{v}</>;
  return (
    <>
      <CountUp value={Number(m[1])} />
      {m[2]}
    </>
  );
}

export function Stats({ t }: { t: Dictionary }) {
  return (
    <section className="bg-band py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid gap-10 text-center sm:grid-cols-3 sm:gap-8">
        {t.stats.items.map((s, i) => (
          <Reveal key={s.k} delay={i * 0.08}>
            <div className="font-display text-[clamp(44px,6vw,64px)] font-semibold tabular-nums text-band-ink">
              <StatValue v={s.v} />
            </div>
            <div className="mt-1 text-sm text-[#9aa9b2]">{s.k}</div>
          </Reveal>
        ))}
      </div>
    </section>
  );
}
