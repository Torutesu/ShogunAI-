import type { Dictionary } from '@/i18n/dictionaries';

// Integration wordmarks — swap for real SVG logos later. The product connects
// to the tools you already use; this strip flows them continuously.
const ROW_A = ['Slack', 'Gmail', 'Notion', 'Linear', 'GitHub', 'Figma', 'Stripe', 'Calendar'];
const ROW_B = ['Drive', 'Zoom', 'Superhuman', 'Raycast', 'Arc', 'Vercel', 'Height', 'Loom'];

function Track({ items, reverse }: { items: string[]; reverse?: boolean }) {
  // Rendered twice so the -50% translate loops seamlessly.
  const doubled = [...items, ...items];
  return (
    <div className={`marquee-track ${reverse ? 'rev' : ''}`}>
      {doubled.map((name, i) => (
        <span
          key={`${name}-${i}`}
          className="mx-8 shrink-0 font-display text-xl font-medium tracking-tight text-faint transition-colors hover:text-muted"
          aria-hidden={i >= items.length}
        >
          {name}
        </span>
      ))}
    </div>
  );
}

export function Marquee({ t }: { t: Dictionary }) {
  return (
    <section className="border-y border-border/60 py-12">
      <div className="container-x">
        <p className="mb-7 text-center text-xs font-medium tracking-[0.02em] text-muted">{t.trust.label}</p>
      </div>
      <div className="group/mq marquee-mask flex flex-col gap-5">
        <Track items={ROW_A} />
        <Track items={ROW_B} reverse />
      </div>
    </section>
  );
}
