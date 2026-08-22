'use client';

import { ArrowLeft, ArrowRight, Check, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

export type CaseScene = { title: string; before: string; lost: string; after: string };

export type CaseLabels = {
  before: string;
  after: string;
  lost: string;
  seeAfter: string;
  seeBefore: string;
  region: string;
  prev: string;
  next: string;
};

const CARD_WIDTH = 'w-[min(900px,86vw)]';

/**
 * The scenes read as a track you move through rather than a stack you scroll
 * past: one card at a time, the neighbours peeking at either edge so it is
 * visible there is more than the one in front of you. Each card still shows a
 * side at a time — today, then the same day with the memory in place.
 */
export function CaseCards({ cases, labels }: { cases: readonly CaseScene[]; labels: CaseLabels }) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);

  const scrollToIndex = useCallback((index: number) => {
    const track = trackRef.current;
    const card = track?.children[index] as HTMLElement | undefined;
    if (!track || !card) return;
    track.scrollTo({ left: card.offsetLeft - (track.clientWidth - card.clientWidth) / 2 });
  }, []);

  // The active card follows the scroll position, not the buttons: a swipe, a
  // trackpad flick and a dot all have to leave the same indicator behind.
  useEffect(() => {
    const track = trackRef.current;
    if (!track) return;

    let frame = 0;
    const sync = () => {
      frame = 0;
      const center = track.scrollLeft + track.clientWidth / 2;
      let nearest = 0;
      let shortest = Number.POSITIVE_INFINITY;
      Array.from(track.children).forEach((child, index) => {
        const card = child as HTMLElement;
        const distance = Math.abs(card.offsetLeft + card.clientWidth / 2 - center);
        if (distance < shortest) {
          shortest = distance;
          nearest = index;
        }
      });
      setActive(nearest);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(sync);
    };

    track.addEventListener('scroll', onScroll, { passive: true });
    sync();
    return () => {
      track.removeEventListener('scroll', onScroll);
      if (frame) cancelAnimationFrame(frame);
    };
  }, [cases]);

  return (
    <div className="mt-14">
      <div
        ref={trackRef}
        role="group"
        aria-label={labels.region}
        tabIndex={0}
        className={`relative flex snap-x snap-mandatory gap-6 overflow-x-auto scroll-smooth rounded-[26px] px-[max(0px,calc((100%-min(900px,86vw))/2))] pb-2 [scrollbar-width:none] focus-visible:ring-2 focus-visible:ring-[#6758ff] focus-visible:outline-none motion-reduce:scroll-auto [&::-webkit-scrollbar]:hidden`}
      >
        {cases.map((scene, index) => (
          <CaseCard
            key={scene.title}
            scene={scene}
            labels={labels}
            index={index}
            total={cases.length}
          />
        ))}
      </div>

      <div className="mt-8 flex items-center justify-center gap-5">
        <TrackButton
          label={labels.prev}
          disabled={active === 0}
          onClick={() => scrollToIndex(active - 1)}
        >
          <ArrowLeft className="size-4" aria-hidden="true" />
        </TrackButton>

        <div className="flex items-center gap-2">
          {cases.map((scene, index) => (
            <button
              key={scene.title}
              type="button"
              onClick={() => scrollToIndex(index)}
              aria-label={scene.title}
              aria-current={index === active}
              className={`h-2 rounded-full transition-[width,background-color] duration-300 motion-reduce:transition-none ${
                index === active ? 'bg-ink w-7' : 'bg-border hover:bg-muted w-2'
              }`}
            />
          ))}
        </div>

        <TrackButton
          label={labels.next}
          disabled={active === cases.length - 1}
          onClick={() => scrollToIndex(active + 1)}
        >
          <ArrowRight className="size-4" aria-hidden="true" />
        </TrackButton>
      </div>
    </div>
  );
}

function TrackButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className="border-border bg-surface text-ink hover:border-ink/25 flex size-10 items-center justify-center rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:border-[color:var(--border)]"
    >
      {children}
    </button>
  );
}

function CaseCard({
  scene,
  labels,
  index,
  total,
}: {
  scene: CaseScene;
  labels: CaseLabels;
  index: number;
  total: number;
}) {
  const [side, setSide] = useState<'before' | 'after'>('before');
  const showingAfter = side === 'after';

  return (
    <article
      aria-label={`${index + 1} / ${total} — ${scene.title}`}
      className={`theme-light-panel border-border bg-surface flex shrink-0 snap-center flex-col overflow-hidden rounded-[26px] border ${CARD_WIDTH}`}
    >
      <header className="border-border flex flex-wrap items-center justify-between gap-4 border-b px-[clamp(22px,3.4vw,40px)] py-6">
        <h3 className="text-ink text-[clamp(19px,1.9vw,25px)] font-semibold tracking-[-0.035em]">{scene.title}</h3>
        <div className="border-border bg-cloud/60 flex shrink-0 rounded-full border p-1" role="group">
          {(['before', 'after'] as const).map((value) => (
            <button
              key={value}
              type="button"
              onClick={() => setSide(value)}
              aria-pressed={side === value}
              className={`rounded-full px-4 py-1.5 text-[12px] font-semibold transition-colors ${
                side === value ? 'bg-ink text-bg' : 'text-muted hover:text-ink'
              }`}
            >
              {value === 'before' ? labels.before : labels.after}
            </button>
          ))}
        </div>
      </header>

      <div className="grid flex-1">
        <div
          aria-hidden={showingAfter}
          className={`col-start-1 row-start-1 px-[clamp(22px,3.4vw,40px)] py-[clamp(26px,3.4vw,40px)] transition-[transform,opacity] duration-500 ease-out motion-reduce:transition-none ${
            showingAfter ? 'pointer-events-none -translate-x-6 opacity-0' : 'translate-x-0 opacity-100'
          }`}
        >
          <p className="text-muted flex items-center gap-2 text-[11px] font-semibold tracking-[0.14em] uppercase">
            <X className="size-4 text-[#ef4d48]" strokeWidth={2.6} aria-hidden="true" />
            {labels.before}
          </p>
          <p className="text-muted mt-5 text-[clamp(15px,1.2vw,17px)] leading-[1.7]">{scene.before}</p>
          <p className="border-border text-muted mt-6 border-t pt-5 text-[14px] leading-[1.6]">
            <span className="font-semibold text-[#ef4d48]">{labels.lost}</span> {scene.lost}
          </p>
          <button
            type="button"
            onClick={() => setSide('after')}
            tabIndex={showingAfter ? -1 : 0}
            className="text-ink mt-7 inline-flex items-center gap-2 text-sm font-semibold underline decoration-[#6758ff]/40 underline-offset-4 hover:decoration-[#6758ff]"
          >
            {labels.seeAfter}
            <ArrowRight className="size-4" aria-hidden="true" />
          </button>
        </div>

        <div
          aria-hidden={!showingAfter}
          className={`theme-soft-section col-start-1 row-start-1 bg-[#f7f4ff] px-[clamp(22px,3.4vw,40px)] py-[clamp(26px,3.4vw,40px)] transition-[transform,opacity] duration-500 ease-out motion-reduce:transition-none ${
            showingAfter ? 'translate-x-0 opacity-100' : 'pointer-events-none translate-x-6 opacity-0'
          }`}
        >
          <p className="text-ink flex items-center gap-2 text-[11px] font-semibold tracking-[0.14em] uppercase">
            <Check className="size-4 text-[#25a65a]" strokeWidth={2.8} aria-hidden="true" />
            {labels.after}
          </p>
          <p className="text-ink mt-5 text-[clamp(15px,1.2vw,17px)] leading-[1.7] font-medium">{scene.after}</p>
          <button
            type="button"
            onClick={() => setSide('before')}
            tabIndex={showingAfter ? 0 : -1}
            className="text-muted hover:text-ink mt-7 inline-flex items-center gap-2 text-sm font-semibold"
          >
            <ArrowLeft className="size-4" aria-hidden="true" />
            {labels.seeBefore}
          </button>
        </div>
      </div>
    </article>
  );
}
