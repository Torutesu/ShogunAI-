import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

export function Trust({ t }: { t: Dictionary }) {
  return (
    <section className="pb-2 pt-12">
      <div className="container-x">
        <Reveal>
          <p className="text-center text-xs font-medium tracking-[0.02em] text-muted">{t.trust.label}</p>
          <div className="mt-5 flex flex-wrap justify-center gap-x-10 gap-y-4">
            {t.trust.tags.map((tag) => (
              <span key={tag} className="font-display text-lg font-medium tracking-tight text-[#b6bec4] sm:text-xl">
                {tag}
              </span>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  );
}
