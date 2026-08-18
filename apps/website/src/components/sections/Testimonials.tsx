import { MessageSquareQuote, Quote } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';

type Item = Dictionary['testimonials']['items'][number];

const TINTS = ['#5865F2', '#004CFC', '#f0b232', '#eb459e', '#23a55a', '#f23f42'];

function tint(seed: string) {
  let n = 0;
  for (const ch of seed) n = (n + ch.charCodeAt(0)) % TINTS.length;
  return TINTS[n];
}

function VoiceAvatar({ item }: { item: Item }) {
  return (
    <span
      className="flex size-12 shrink-0 items-center justify-center rounded-2xl text-sm font-semibold text-white shadow-[0_8px_20px_rgba(9,11,12,0.08)]"
      style={{ background: tint(item.author) }}
    >
      {item.initials}
    </span>
  );
}

function Meta({ item }: { item: Item }) {
  return (
    <div className="min-w-0">
      <div className="text-[15px] font-semibold text-ink">{item.author}</div>
      <div className="mt-0.5 text-[13px] text-muted">{item.role}</div>
      <div className="mt-1 text-[12px] text-faint">{item.time}</div>
    </div>
  );
}

function FeaturedVoice({ item }: { item: Item }) {
  return (
    <div className="relative overflow-hidden rounded-[34px] border border-white/60 bg-[linear-gradient(135deg,rgba(255,255,255,0.94),rgba(245,252,255,0.9))] p-7 shadow-[0_22px_70px_rgba(9,11,12,0.08)] backdrop-blur md:p-9">
      <div
        aria-hidden="true"
        className="absolute inset-0 bg-[radial-gradient(circle_at_top_right,rgba(0,76,252,0.12),transparent_28%),radial-gradient(circle_at_bottom_left,rgba(255,255,255,0.95),transparent_35%)]"
      />
      <div className="relative">
        <div className="flex items-center justify-between gap-4">
          <span className="inline-flex items-center gap-2 rounded-full border border-[#cfefff] bg-[#ebf9ff] px-3 py-1.5 text-[12px] font-semibold text-[#0a607f]">
            <MessageSquareQuote className="size-3.5" strokeWidth={2.1} />
            Early access feedback
          </span>
          <span className="text-[42px] font-light leading-none text-ink/80">𝕏</span>
        </div>

        <Quote className="mt-8 size-9 text-[#b9d9e8]" strokeWidth={2.2} />
        <p className="mt-5 max-w-[30ch] font-display text-[clamp(24px,3.4vw,44px)] font-semibold leading-[1.04] tracking-[-0.035em] text-ink">
          {item.text}
        </p>

        <div className="mt-8 flex items-center gap-4">
          <VoiceAvatar item={item} />
          <Meta item={item} />
        </div>
      </div>
    </div>
  );
}

function VoiceCard({ item }: { item: Item }) {
  return (
    <figure className="lift relative overflow-hidden rounded-[28px] border border-border/80 bg-white/88 p-6 shadow-[0_18px_50px_rgba(9,11,12,0.06)]">
      <div
        aria-hidden="true"
        className="absolute inset-x-0 top-0 h-px bg-[linear-gradient(90deg,transparent,rgba(0,76,252,0.34),transparent)]"
      />
      <Quote className="mb-5 size-8 text-[#d7dde2]" strokeWidth={2.2} />
      <p className="text-[15px] leading-[1.8] text-ink">{item.text}</p>
      <div className="mt-6 flex items-end justify-between gap-4">
        <div className="flex gap-3">
          <VoiceAvatar item={item} />
          <Meta item={item} />
        </div>
        <span className="text-[38px] font-light leading-none text-ink/82">{item.platform === 'discord' ? '✦' : '𝕏'}</span>
      </div>
    </figure>
  );
}

export function Testimonials({ t }: { t: Dictionary }) {
  const items = t.testimonials.items;
  const featured = items[0];
  const rest = items.slice(1);

  return (
    <section id="testimonials" className="theme-soft-section scroll-mt-20 bg-[#fffdf5] py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-14 max-w-[720px] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.testimonials.eyebrow}</p>
          <h2 className="testimonials-title mx-auto mt-4 max-w-[12ch] text-balance font-display text-[clamp(34px,6vw,72px)] font-semibold leading-[1.02] tracking-[-0.045em]">
            {t.testimonials.title}
          </h2>
          <p className="testimonial-sub mx-auto mt-5 max-w-[42rem] text-[17px] leading-relaxed text-muted">{t.testimonials.sub}</p>
        </Reveal>

        <div className="grid gap-6 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,0.8fr)]">
          <Reveal>
            <FeaturedVoice item={featured} />
          </Reveal>

          <div className="grid gap-6">
            {rest.map((item, i) => (
              <Reveal key={item.author} delay={i * 0.06 + 0.04}>
                <VoiceCard item={item} />
              </Reveal>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
