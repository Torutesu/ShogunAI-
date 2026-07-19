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

  /* --- Y Combinator companies --- */
  Airbnb: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M12 2c-1.3 0-2.2 1-3 2.6-2 3.7-5 9.6-5 12.3A5.1 5.1 0 0 0 12 21a5.1 5.1 0 0 0 8-4.1c0-2.7-3-8.6-5-12.3C14.2 3 13.3 2 12 2Zm0 2.4c.5 0 .9.5 1.6 1.9.4.8.8 1.6.8 2.4A2.4 2.4 0 0 1 12 11a2.4 2.4 0 0 1-2.4-2.3c0-.8.4-1.6.8-2.4.7-1.4 1.1-1.9 1.6-1.9Zm0 8.5a2.4 2.4 0 0 1 2 3.7l-2 3-2-3a2.4 2.4 0 0 1 2-3.7Z" />
    </svg>
  ),
  Coinbase: (
    <svg viewBox="0 0 24 24" fill="currentColor" fillRule="evenodd" className="size-[22px]">
      <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm-1.6 7a1.4 1.4 0 0 0-1.4 1.4v3.2a1.4 1.4 0 0 0 1.4 1.4h3.2a1.4 1.4 0 0 0 1.4-1.4v-3.2a1.4 1.4 0 0 0-1.4-1.4z" />
    </svg>
  ),
  Dropbox: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M7 2 1 6l6 4 6-4-6-4Zm10 0-6 4 6 4 6-4-6-4ZM1 14l6 4 6-4-6-4-6 4Zm16-4-6 4 6 4 6-4-6-4ZM7 19.2l6-4 6 4-6 3.8-6-3.8Z" />
    </svg>
  ),
  DoorDash: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M2 8.5h13.2a4.75 4.75 0 0 1 0 9.5H7.5l3-3h4.7a1.75 1.75 0 0 0 0-3.5H5l-3-3Z" />
    </svg>
  ),
  Reddit: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} className="size-[22px]">
      <circle cx="17.6" cy="5.4" r="1.5" />
      <path d="M13 8.4 14 4l3.6.9" strokeLinejoin="round" />
      <ellipse cx="12" cy="14" rx="8" ry="5.4" />
      <circle cx="9" cy="13.8" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="15" cy="13.8" r="1.15" fill="currentColor" stroke="none" />
      <path d="M9.4 16.4c1.5 1.1 3.7 1.1 5.2 0" />
    </svg>
  ),
  Twitch: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M4 3 3 6.5V19h4v2.5L9.5 19H13l6-6V3H4Zm13.5 9.5-2.5 2.5h-3.5L9 17v-2.5H6.5V5h11v7.5ZM15 7v4h-1.5V7H15Zm-4 0v4H9.5V7H11Z" />
    </svg>
  ),
  Instacart: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M14.5 3a2.6 2.6 0 0 0-2.3 1.4 3 3 0 0 1 1.9 1.8 2.6 2.6 0 0 1 2.4.3A2.6 2.6 0 0 0 14.5 3Zm-2.2 4.7c-1.4-1.4-3.7-.5-5.8 1.6-2.6 2.6-5.6 7.9-4.3 9.2s6.6-1.7 9.2-4.3c2.1-2.1 3-4.4 1.6-5.8l3.3-3.3-.9-.9-3.4 3.5Z" />
    </svg>
  ),
  Docker: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="M3 10h2.6v2.6H3zM6 10h2.6v2.6H6zM9 10h2.6v2.6H9zM6 7h2.6v2.6H6zM9 7h2.6v2.6H9zM12 7h2.6v2.6H12zM12 10h2.6v2.6H12z" />
      <path d="M1.5 13.5h19c-.4 2.2-2.3 4.8-6 5.4-4 .7-9.5.2-11.5-2.5-.9-1.2-1.4-1.9-1.5-2.9Z" />
    </svg>
  ),
  GitLab: (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-[22px]">
      <path d="m12 21 3.6-11.1H8.4L12 21Zm0 0L3.2 10.4 12 21Zm0 0 8.8-10.6L12 21ZM3.2 10.4 1.7 15c-.1.4 0 .8.4 1L12 21 3.2 10.4Zm0 0L4.8 5c.1-.3.5-.3.6 0l1.6 5.4H3.2Zm17.6 0L12 21l9.9-5c.4-.2.5-.6.4-1l-1.5-4.6Zm0 0h-3.8l1.6-5.4c.1-.3.5-.3.6 0l1.6 5.4Z" />
    </svg>
  ),
};

const ROW_A = ['Slack', 'Notion', 'Linear', 'GitHub', 'Figma', 'Airbnb', 'Coinbase', 'Dropbox', 'Instacart'];
const ROW_B = ['Stripe', 'Vercel', 'Zoom', 'Loom', 'Gmail', 'DoorDash', 'Reddit', 'Docker', 'GitLab'];

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
