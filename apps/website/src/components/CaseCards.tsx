'use client';

import { ArrowLeft, ArrowRight, Check, X } from 'lucide-react';
import { useState } from 'react';
import { Reveal } from '@/components/animations/Reveal';

export type CaseScene = { title: string; before: string; lost: string; after: string };

export type CaseLabels = {
  before: string;
  after: string;
  lost: string;
  seeAfter: string;
  seeBefore: string;
};

/**
 * One scene per card, shown a side at a time: today, then the same day with the
 * memory in place. The panels sit in the same grid cell so the card keeps the
 * height of the taller side and nothing jumps when it slides.
 *
 * Each card rises in as it reaches the viewport, so scrolling the section hands
 * you the scenes one at a time. No stagger delay: a card is most of a screen
 * tall, so they arrive one by one already — a delay would only hold a card back
 * after it is in view.
 */
export function CaseCards({ cases, labels }: { cases: readonly CaseScene[]; labels: CaseLabels }) {
  return (
    <div className="mx-auto mt-14 grid max-w-[900px] gap-6">
      {cases.map((scene) => (
        <Reveal key={scene.title} y={26}>
          <CaseCard scene={scene} labels={labels} />
        </Reveal>
      ))}
    </div>
  );
}

function CaseCard({ scene, labels }: { scene: CaseScene; labels: CaseLabels }) {
  const [side, setSide] = useState<'before' | 'after'>('before');
  const showingAfter = side === 'after';

  return (
    <article className="theme-light-panel border-border bg-surface overflow-hidden rounded-[26px] border">
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

      <div className="grid">
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
