import { Plus } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { JsonLd } from '@/components/seo/JsonLd';
import type { Dictionary } from '@/i18n/dictionaries';

export function FAQ({ t }: { t: Dictionary }) {
  const faqSchema = {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: t.faq.items.map(([q, a]) => ({
      '@type': 'Question',
      name: q,
      acceptedAnswer: { '@type': 'Answer', text: a },
    })),
  };

  return (
    <section id="faq" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <JsonLd data={faqSchema} />
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[40ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.faq.eyebrow}</p>
          <h2 className="faq-title mt-3.5 font-display text-[clamp(24px,5.5vw,46px)] font-semibold leading-[1.08] tracking-[-0.02em] text-balance">
            {t.faq.title}
          </h2>
        </Reveal>

        <div className="mx-auto grid max-w-[760px] gap-3">
          {t.faq.items.map(([q, a], i) => (
            <Reveal key={q} delay={i * 0.04}>
              <details className="group rounded-xl border border-border bg-surface px-5 transition-shadow open:shadow-[var(--shadow-card)]">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-4 py-[18px] text-base font-semibold [&::-webkit-details-marker]:hidden">
                  {q}
                  <span className="flex size-[22px] shrink-0 items-center justify-center rounded-full bg-sky-soft text-accent transition-transform group-open:rotate-45">
                    <Plus className="size-3.5" strokeWidth={3} />
                  </span>
                </summary>
                <p className="pb-[18px] text-[15px] leading-relaxed text-muted">{a}</p>
              </details>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
