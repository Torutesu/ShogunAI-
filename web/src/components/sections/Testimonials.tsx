import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

type Item = Dictionary['testimonials']['items'][number];

// Deterministic avatar tint from the author name (no Math.random at render).
const TINTS = ['#5865F2', '#00a6f4', '#f0b232', '#eb459e', '#23a55a', '#f23f42'];
function tint(seed: string) {
  let n = 0;
  for (const ch of seed) n = (n + ch.charCodeAt(0)) % TINTS.length;
  return TINTS[n];
}

function DiscordIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 18" className={className} fill="currentColor" aria-hidden="true">
      <path d="M20.3 1.5A19.8 19.8 0 0015.4 0l-.25.5a18.3 18.3 0 015.4 1.7 13.6 13.6 0 00-16.1 0A18.3 18.3 0 019.85.5L9.6 0A19.8 19.8 0 004.7 1.5C1.6 6.1.75 10.6 1.2 15a19.9 19.9 0 006 3 14.3 14.3 0 001.3-2.1 12.9 12.9 0 01-2-1c.17-.12.33-.25.5-.38a9.7 9.7 0 008.5 0l.5.38a12.9 12.9 0 01-2 1A14.3 14.3 0 0016.8 18a19.9 19.9 0 006-3c.55-5.1-.85-9.55-3.5-13.5zM8.5 12.2c-1.2 0-2.15-1.08-2.15-2.4S7.3 7.4 8.5 7.4s2.17 1.08 2.15 2.4c0 1.32-.95 2.4-2.15 2.4zm7 0c-1.2 0-2.15-1.08-2.15-2.4s.95-2.4 2.15-2.4 2.17 1.08 2.15 2.4c0 1.32-.95 2.4-2.15 2.4z" />
    </svg>
  );
}

function LinkedInIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden="true">
      <path d="M20.45 20.45h-3.55v-5.57c0-1.33-.02-3.04-1.85-3.04-1.85 0-2.13 1.45-2.13 2.94v5.67H9.36V9h3.41v1.56h.05c.48-.9 1.64-1.85 3.37-1.85 3.6 0 4.27 2.37 4.27 5.45v6.29zM5.34 7.43a2.06 2.06 0 110-4.12 2.06 2.06 0 010 4.12zM7.12 20.45H3.56V9h3.56v11.45zM22.22 0H1.77C.79 0 0 .77 0 1.72v20.56C0 23.23.79 24 1.77 24h20.45c.98 0 1.78-.77 1.78-1.72V1.72C24 .77 23.2 0 22.22 0z" />
    </svg>
  );
}

function DiscordCard({ item }: { item: Item }) {
  return (
    <figure className="overflow-hidden rounded-xl border border-[#1e1f22] bg-[#313338] shadow-[var(--shadow-float)]">
      <div className="flex items-center gap-2 border-b border-black/20 bg-[#2b2d31] px-4 py-2 text-[#b5bac1]">
        <DiscordIcon className="h-3.5 w-auto text-[#5865F2]" />
        <span className="text-xs font-semibold">Discord</span>
        <span className="ml-auto text-[11px] text-[#949ba4]">#early-access</span>
      </div>
      <div className="flex gap-3 px-4 py-3.5">
        <span
          className="mt-0.5 flex size-10 shrink-0 items-center justify-center rounded-full text-sm font-semibold text-white"
          style={{ background: tint(item.author) }}
        >
          {item.initials}
        </span>
        <div className="min-w-0">
          <div className="flex items-baseline gap-2">
            <span className="text-[15px] font-semibold text-white">{item.author}</span>
            <span className="text-[11px] text-[#949ba4]">{item.time}</span>
          </div>
          <p className="mt-0.5 text-sm leading-relaxed text-[#dbdee1]">{item.text}</p>
        </div>
      </div>
    </figure>
  );
}

function LinkedInCard({ item }: { item: Item }) {
  return (
    <figure className="overflow-hidden rounded-xl border border-border bg-surface shadow-[var(--shadow-card)]">
      <div className="flex items-center gap-2 border-b border-border bg-cloud px-4 py-2 text-[#0a66c2]">
        <LinkedInIcon className="h-3.5 w-auto" />
        <span className="text-xs font-semibold text-ink">LinkedIn</span>
        <span className="ml-auto text-[11px] text-muted">{item.time}</span>
      </div>
      <div className="px-4 py-3.5">
        <div className="flex items-center gap-3">
          <span
            className="flex size-10 shrink-0 items-center justify-center rounded-full text-sm font-semibold text-white"
            style={{ background: tint(item.author) }}
          >
            {item.initials}
          </span>
          <div>
            <div className="text-[15px] font-semibold text-ink">{item.author}</div>
            <div className="text-xs text-muted">{item.role}</div>
          </div>
        </div>
        <p className="mt-3 rounded-2xl rounded-tl-sm bg-cloud px-4 py-3 text-sm leading-relaxed text-ink">{item.text}</p>
      </div>
    </figure>
  );
}

export function Testimonials({ t }: { t: Dictionary }) {
  const items = t.testimonials.items;
  return (
    <section id="testimonials" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[48ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.testimonials.eyebrow}</p>
          <h2 className="mt-3.5 font-display text-[clamp(30px,4vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.testimonials.title}
          </h2>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">{t.testimonials.sub}</p>
        </Reveal>

        {/* Masonry-ish columns so cards of different heights pack tightly */}
        <div className="mx-auto max-w-[860px] gap-5 [column-count:1] sm:[column-count:2]">
          {items.map((item, i) => (
            <Reveal key={item.author} delay={(i % 2) * 0.08} className="mb-5 break-inside-avoid">
              {item.platform === 'discord' ? <DiscordCard item={item} /> : <LinkedInCard item={item} />}
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
