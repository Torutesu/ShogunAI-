import { Reveal } from '@/components/animations/Reveal';
import { Card } from '@/components/ui/card';
import type { Dictionary } from '@/i18n/dictionaries';

export function How({ t }: { t: Dictionary }) {
  return (
    <section id="how" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[44ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.how.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.how.title}
          </h2>
        </Reveal>
        <div className="grid gap-6 md:grid-cols-3">
          {t.how.steps.map((s, i) => (
            <Reveal key={s.title} delay={i * 0.08}>
              <Card className="lift h-full">
                <div className="mb-3.5 font-mono text-sm text-accent">{String(i + 1).padStart(2, '0')}</div>
                <h3 className="font-display text-2xl font-semibold">{s.title}</h3>
                <p className="mt-2.5 text-sm leading-relaxed text-muted">{s.body}</p>
              </Card>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
