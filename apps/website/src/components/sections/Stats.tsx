import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

export function Stats({ t }: { t: Dictionary }) {
  return (
    <section className="bg-band py-[clamp(56px,9vw,112px)]">
      <div className="container-x grid gap-8 text-center md:grid-cols-3">
        {t.stats.items.map((s, i) => (
          <Reveal key={s.k} delay={i * 0.08}>
            <div className="font-display text-[clamp(44px,6vw,64px)] font-semibold tabular-nums text-band-ink">
              {s.v}
            </div>
            <div className="text-sm text-[#9aa9b2]">{s.k}</div>
          </Reveal>
        ))}
      </div>
    </section>
  );
}
