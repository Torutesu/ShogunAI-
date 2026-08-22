'use client';

import { ArrowRight, CalendarDays, Check, FileText, ListTodo, Mail, MessageSquare, X } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';

export type CaseScene = { title: string; before: string; lost: string; after: string };

export type CaseLabels = {
  before: string;
  after: string;
  lost: string;
};

/** Generic enough for every persona: a day's traffic, whatever the work is. */
const FRAGMENT_ICONS = [MessageSquare, FileText, CalendarDays, ListTodo, Mail] as const;

/**
 * One scene per card, both sides on the face of it: what the day costs today,
 * then the same day with the memory in place. Nothing is behind a control —
 * the point of the section is the comparison, so hiding half of it behind a
 * toggle put the answer one click away from the question.
 *
 * The cards rise in as they reach the viewport, staggered across the row.
 */
export function CaseCards({ cases, labels }: { cases: readonly CaseScene[]; labels: CaseLabels }) {
  return (
    <div className="mt-14 grid gap-5 sm:grid-cols-2 xl:grid-cols-3">
      {cases.map((scene, index) => (
        <Reveal key={scene.title} className="h-full" delay={index * 0.08} y={26}>
          <CaseCard scene={scene} labels={labels} index={index} />
        </Reveal>
      ))}
    </div>
  );
}

function CaseCard({ scene, labels, index }: { scene: CaseScene; labels: CaseLabels; index: number }) {
  return (
    <article className="theme-light-panel border-border bg-surface flex h-full flex-col overflow-hidden rounded-[26px] border p-4">
      <SceneDiagram index={index} />

      <div className="flex flex-1 flex-col px-2 pt-6 pb-2">
        <h3 className="text-ink text-[clamp(18px,1.4vw,21px)] leading-[1.35] font-semibold tracking-[-0.03em]">
          {scene.title}
        </h3>

        <p className="text-muted mt-5 flex items-center gap-2 text-[11px] font-semibold tracking-[0.14em] uppercase">
          <X className="size-4 text-[#ef4d48]" strokeWidth={2.6} aria-hidden="true" />
          {labels.before}
        </p>
        <p className="text-muted mt-3 text-[14px] leading-[1.75]">{scene.before}</p>
        <p className="border-border text-muted mt-5 border-t pt-4 text-[13px] leading-[1.6]">
          <span className="font-semibold text-[#ef4d48]">{labels.lost}</span> {scene.lost}
        </p>

        <div className="mt-auto pt-6">
          <div className="theme-soft-section rounded-[18px] bg-[#f7f4ff] p-4">
            <p className="text-ink flex items-center gap-2 text-[11px] font-semibold tracking-[0.14em] uppercase">
              <Check className="size-4 text-[#25a65a]" strokeWidth={2.8} aria-hidden="true" />
              {labels.after}
            </p>
            <p className="text-ink mt-3 text-[14px] leading-[1.75] font-medium">{scene.after}</p>
          </div>
        </div>
      </div>
    </article>
  );
}

/**
 * The same figure on every card — scattered fragments on the left, one settled
 * record on the right — with only the icons rotating by index, so five cards in
 * a row do not read as five copies of one picture. Decorative: the card's text
 * carries the meaning, so it is hidden from assistive tech.
 */
function SceneDiagram({ index }: { index: number }) {
  const fragments = [0, 1, 2].map((offset) => FRAGMENT_ICONS[(index + offset) % FRAGMENT_ICONS.length]);
  const offsets = [7, -5, 4];
  const widths = [26, 34, 20];

  return (
    <div
      aria-hidden="true"
      className="bg-cloud border-border flex h-[132px] items-center justify-center gap-4 overflow-hidden rounded-[20px] border px-5"
    >
      <div className="flex flex-col gap-2">
        {fragments.map((Icon, i) => (
          <span
            key={i}
            style={{ transform: `translateX(${offsets[i]}px)`, opacity: 1 - i * 0.14 }}
            className="border-border bg-surface flex items-center gap-2 rounded-[10px] border px-2.5 py-1.5 shadow-[0_2px_6px_rgba(11,18,32,0.05)]"
          >
            <Icon className="text-faint size-3.5" strokeWidth={2} />
            <span className="bg-border block h-1 rounded-full" style={{ width: widths[i] }} />
          </span>
        ))}
      </div>

      <ArrowRight className="size-4 shrink-0 text-[#6758ff]" strokeWidth={2.4} />

      <div className="flex items-center gap-2.5 rounded-[12px] border border-[#6758ff]/25 bg-[#f7f4ff] px-3 py-2.5 shadow-[0_6px_16px_rgba(103,88,255,0.12)]">
        <Check className="size-4 shrink-0 text-[#25a65a]" strokeWidth={2.8} />
        <span className="flex flex-col gap-1.5">
          <span className="block h-1 w-14 rounded-full bg-[#6758ff]/35" />
          <span className="block h-1 w-9 rounded-full bg-[#6758ff]/20" />
        </span>
      </div>
    </div>
  );
}
