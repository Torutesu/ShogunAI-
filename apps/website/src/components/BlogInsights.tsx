import type { Locale } from '@/i18n/config';

const COPY = {
  en: {
    eyebrow: 'Insights',
    title: 'AI market notes',
    intro: 'A focused space for market signals, investor and CEO perspectives, and research around the next layer of AI work.',
    tab: 'AI',
    items: ['Market signals', 'Investor & CEO perspectives', 'Research notes'],
    soon: 'Editorial section coming soon',
  },
  ja: {
    eyebrow: 'インサイト',
    title: 'AIマーケットノート',
    intro: 'AI市場の動向、投資家やCEOの視点、これからのAIワークを支えるリサーチをまとめる場所です。',
    tab: 'AI',
    items: ['市場のシグナル', '投資家・CEOの視点', 'リサーチノート'],
    soon: '編集セクションを準備中',
  },
  es: {
    eyebrow: 'Insights',
    title: 'Notas del mercado de IA',
    intro: 'Un espacio para señales del mercado, perspectivas de inversores y CEOs, e investigación sobre la próxima capa del trabajo con IA.',
    tab: 'IA',
    items: ['Señales del mercado', 'Perspectivas de inversores y CEOs', 'Notas de investigación'],
    soon: 'Sección editorial próximamente',
  },
  de: {
    eyebrow: 'Insights',
    title: 'Notizen zum KI-Markt',
    intro: 'Ein Bereich für Marktsignale, Perspektiven von Investoren und CEOs sowie Forschung zur nächsten Ebene der KI-Arbeit.',
    tab: 'KI',
    items: ['Marktsignale', 'Perspektiven von Investoren und CEOs', 'Forschungsnotizen'],
    soon: 'Redaktioneller Bereich folgt',
  },
} as const;

export function BlogInsights({ locale }: { locale: Locale }) {
  const copy = COPY[locale];

  return (
    <aside className="lg:sticky lg:top-24 lg:self-start" aria-label={copy.eyebrow}>
      <div className="rounded-2xl border border-border bg-surface p-5 shadow-[0_16px_44px_rgba(19,55,77,0.05)] sm:p-6">
        <p className="text-xs font-semibold uppercase tracking-[0.12em] text-accent">{copy.eyebrow}</p>
        <h2 className="mt-3 font-display text-xl font-semibold tracking-tight">{copy.title}</h2>
        <p className="mt-3 text-sm leading-relaxed text-muted">{copy.intro}</p>

        <div className="mt-6 border-b border-border pb-2">
          <span className="inline-flex rounded-full bg-ink px-3 py-1.5 text-xs font-semibold text-on-ink">{copy.tab}</span>
        </div>

        <nav className="mt-3" aria-label={`${copy.eyebrow} topics`}>
          <ul className="divide-y divide-border">
            {copy.items.map((item) => (
              <li key={item} className="py-3 text-sm font-medium text-muted">
                {item}
              </li>
            ))}
          </ul>
        </nav>

        <p className="mt-5 text-xs leading-relaxed text-muted/80">{copy.soon}</p>
      </div>
    </aside>
  );
}
