import type { Dictionary } from '@/i18n/dictionaries';

/* Simplified monochrome brand marks (currentColor). Swap for official SVGs. */
const MARKS: Record<string, React.ReactNode> = {
  Slack: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <rect x="4" y="10.5" width="6" height="3" rx="1.5" />
      <rect x="10.5" y="14" width="3" height="6" rx="1.5" />
      <rect x="14" y="10.5" width="6" height="3" rx="1.5" />
      <rect x="10.5" y="4" width="3" height="6" rx="1.5" />
    </svg>
  ),
  Notion: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} className="size-[22px]">
      <rect x="3.5" y="3.5" width="17" height="17" rx="4" />
      <path d="M8.5 16V8l7 8V8" strokeLinejoin="round" />
    </svg>
  ),
  Linear: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M2.6 13.7a9.5 9.5 0 0 0 7.7 7.7zM2.1 11 13 21.9c.85-.16 1.66-.43 2.4-.78L2.9 8.6c-.35.74-.62 1.55-.78 2.4zM3.7 6.6 17.4 20.3a9.6 9.6 0 0 0 1.6-1.3L5 5a9.6 9.6 0 0 0-1.3 1.6zM6.7 3.6a9.5 9.5 0 0 1 13.7 13.7z" />
    </svg>
  ),
  GitHub: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M12 1.5A10.5 10.5 0 0 0 8.68 21.97c.52.1.71-.23.71-.5v-1.76c-2.9.63-3.52-1.4-3.52-1.4-.47-1.2-1.16-1.52-1.16-1.52-.95-.65.07-.64.07-.64 1.05.07 1.6 1.08 1.6 1.08.93 1.6 2.45 1.14 3.05.87.1-.68.36-1.14.66-1.4-2.32-.26-4.76-1.16-4.76-5.16 0-1.14.4-2.07 1.07-2.8-.1-.26-.46-1.32.1-2.75 0 0 .87-.28 2.85 1.07a9.8 9.8 0 0 1 5.18 0c1.98-1.35 2.85-1.07 2.85-1.07.56 1.43.2 2.49.1 2.75.67.73 1.07 1.66 1.07 2.8 0 4.01-2.45 4.9-4.78 5.16.38.32.71.95.71 1.92v2.85c0 .28.19.61.72.5A10.5 10.5 0 0 0 12 1.5z" />
    </svg>
  ),
  Figma: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M8.5 24a3.75 3.75 0 0 0 3.75-3.75V16.5H8.5a3.75 3.75 0 0 0 0 7.5z" />
      <path d="M4.75 12a3.75 3.75 0 0 1 3.75-3.75h3.75v7.5H8.5A3.75 3.75 0 0 1 4.75 12z" />
      <path d="M4.75 4.5A3.75 3.75 0 0 1 8.5.75h3.75v7.5H8.5A3.75 3.75 0 0 1 4.75 4.5z" />
      <path d="M12.25.75H16a3.75 3.75 0 0 1 0 7.5h-3.75z" />
      <circle cx="16" cy="12" r="3.75" />
    </svg>
  ),
  Stripe: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M13.4 9.3c0-.9.74-1.24 1.96-1.24 1.75 0 3.96.53 5.71 1.48V4.9A15 15 0 0 0 15.36 4C11.2 4 8.43 6.18 8.43 9.82c0 5.67 7.8 4.76 7.8 7.2 0 1.05-.9 1.4-2.2 1.4-1.9 0-4.35-.8-6.28-1.86v5.54A15.7 15.7 0 0 0 14.03 23c4.27 0 7.2-2.11 7.2-5.8 0-6.11-7.83-5.02-7.83-7.9z" />
    </svg>
  ),
  Vercel: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M12 3 22.4 21H1.6z" />
    </svg>
  ),
  Zoom: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <rect x="2" y="6.5" width="14" height="11" rx="3" />
      <path d="M17 10.2l5-2.7v9l-5-2.7z" />
    </svg>
  ),
  Loom: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} className="size-[22px]">
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="3.2" fill="currentColor" stroke="none" />
      <path d="M12 3v4M12 17v4M3 12h4M17 12h4" />
    </svg>
  ),
  Gmail: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} className="size-[22px]">
      <rect x="2.5" y="5" width="19" height="14" rx="2.5" />
      <path d="M3 6.5 12 13l9-6.5" strokeLinejoin="round" />
    </svg>
  ),
};

const ROW_A = ['Slack', 'Notion', 'Linear', 'GitHub', 'Figma'];
const ROW_B = ['Stripe', 'Vercel', 'Zoom', 'Loom', 'Gmail'];

function Track({ items, reverse }: { items: string[]; reverse?: boolean }) {
  const doubled = [...items, ...items];
  return (
    <div className={`marquee-track ${reverse ? 'rev' : ''}`}>
      {doubled.map((name, i) => (
        <span
          key={`${name}-${i}`}
          className="mx-7 inline-flex shrink-0 items-center gap-2.5 text-[19px] font-semibold tracking-tight text-faint transition-colors hover:text-muted"
          aria-hidden={i >= items.length}
        >
          {MARKS[name]}
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
