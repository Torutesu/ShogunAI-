'use client';

import { useEffect, useRef, useState } from 'react';
import { ArrowRight, Check, Command, Mic, Play, Search, Sparkles, X } from 'lucide-react';
import type { Dictionary } from '@/i18n/dictionaries';

type Demo = Dictionary['heroDemo'];
type RowState = 'idle' | 'approved' | 'dismissed';

/** Mock product surface for the hero. Everything here is local state: no network,
  * no real capture, no timers that outlive the component. The point is that a
  * visitor can press the things the copy talks about — the L3 approval gate, the
  * Option-key draft, recall with provenance, a meeting that ends in minutes. */
export function HeroDemo({ d, cta, live, macos }: { d: Demo; cta: string; live: string; macos: string }) {
  const [tab, setTab] = useState(0);
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);
  useEffect(() => () => timers.current.forEach(clearTimeout), []);
  const after = (ms: number, fn: () => void) => {
    timers.current.push(setTimeout(fn, ms));
  };

  return (
    <div className="hero-demo-shell relative overflow-hidden rounded-[28px] border border-white/70 bg-[#0a1533]/90 p-3 shadow-[0_35px_90px_rgba(0,38,142,0.28)] backdrop-blur-xl sm:p-4">
      <div className="absolute inset-x-0 top-0 h-px bg-white/50" />
      <div className="flex items-center justify-between px-2 pb-3 text-[11px] font-medium text-white/62">
        <span className="flex items-center gap-2">
          <span className="size-2 rounded-full bg-[#7ee0af] shadow-[0_0_12px_#7ee0af]" /> {live}
        </span>
        <span>{macos}</span>
      </div>

      <div className="rounded-[20px] border border-white/10 bg-[#10224d]/90 p-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.07)] sm:p-5">
        <div className="flex flex-wrap items-center gap-1.5" role="tablist" aria-label={d.hint}>
          {d.tabs.map((label, i) => (
            <button
              key={label}
              type="button"
              role="tab"
              aria-selected={tab === i}
              onClick={() => setTab(i)}
              className={`min-h-8 rounded-full px-3 py-1.5 text-[11px] font-semibold transition-colors ${
                tab === i ? 'bg-white text-[#10224d]' : 'bg-white/8 text-white/70 hover:bg-white/14 hover:text-white'
              }`}
            >
              {label}
            </button>
          ))}
          <span className="ml-auto hidden items-center gap-1 text-[10px] text-white/40 sm:flex">
            <Sparkles className="size-3" /> {d.hint}
          </span>
        </div>

        <div className="mt-4 min-h-[268px]">
          {tab === 0 && <TodayPane d={d} />}
          {tab === 1 && <RecallPane d={d} after={after} />}
          {tab === 2 && <DraftPane d={d} after={after} />}
          {tab === 3 && <MeetingPane d={d} after={after} />}
        </div>
      </div>

      <a
        href="#get-started"
        className="mt-3 flex min-h-11 w-full items-center justify-center gap-2 rounded-full border border-white/15 bg-white/10 px-4 text-sm font-semibold text-white transition-colors hover:bg-white/[0.18]"
      >
        <Play className="size-3.5 fill-current" />
        {cta}
        <ArrowRight className="size-3.5" />
      </a>
    </div>
  );
}

function TodayPane({ d }: { d: Demo }) {
  const [rows, setRows] = useState<RowState[]>(() => d.today.rows.map(() => 'idle'));
  const set = (i: number, v: RowState) => setRows((prev) => prev.map((p, k) => (k === i ? v : p)));

  return (
    <div>
      <h2 className="font-display text-[20px] font-medium tracking-[-0.035em] text-white sm:text-[23px]">{d.today.heading}</h2>
      <div className="mt-4 grid grid-cols-3 gap-2 border-y border-white/10 py-3.5">
        {d.today.stats.map(([label, value]) => (
          <div key={label} className="min-w-0">
            <p className="font-display text-xl font-medium text-white">{value}</p>
            <p className="mt-1 truncate text-[10px] text-white/52">{label}</p>
          </div>
        ))}
      </div>
      <p className="mt-3.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-[#9db2ff]">{d.today.queued}</p>
      <div className="mt-2 space-y-2">
        {d.today.rows.map((row, i) => (
          <div key={row.label} className="flex items-center gap-3 rounded-xl border border-white/8 bg-white/[0.055] px-3 py-2.5">
            <span className={`flex size-7 shrink-0 items-center justify-center rounded-lg text-[11px] font-bold ${i === 0 ? 'bg-[#004cfc] text-white' : 'bg-[#f0a76c] text-[#312117]'}`}>
              {row.icon}
            </span>
            <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-white/88">{row.label}</span>
            {rows[i] === 'idle' ? (
              <span className="flex shrink-0 items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => set(i, 'approved')}
                  className="min-h-7 rounded-full bg-[#87e5b4] px-2.5 text-[10px] font-bold text-[#0a2b1c] transition-transform hover:-translate-y-px"
                >
                  {d.today.approve}
                </button>
                <button
                  type="button"
                  onClick={() => set(i, 'dismissed')}
                  aria-label={d.today.later}
                  title={d.today.later}
                  className="flex size-7 items-center justify-center rounded-full bg-white/10 text-white/60 transition-colors hover:bg-white/20 hover:text-white"
                >
                  <X className="size-3" />
                </button>
              </span>
            ) : (
              <span className={`shrink-0 text-[10px] font-semibold ${rows[i] === 'approved' ? 'text-[#87e5b4]' : 'text-white/45'}`}>
                {rows[i] === 'approved' ? d.today.approved : d.today.dismissed}
              </span>
            )}
          </div>
        ))}
      </div>
      <p className="mt-3 text-[10px] leading-relaxed text-white/45">{d.today.gate}</p>
    </div>
  );
}

function RecallPane({ d, after }: { d: Demo; after: (ms: number, fn: () => void) => void }) {
  const [picked, setPicked] = useState<number | null>(null);
  const [thinking, setThinking] = useState(false);
  const ask = (i: number) => {
    setPicked(null);
    setThinking(true);
    after(420, () => {
      setThinking(false);
      setPicked(i);
    });
  };
  const answer = picked === null ? null : d.recall.answers[picked];

  return (
    <div>
      <div className="flex items-center gap-2 rounded-xl border border-white/12 bg-white/[0.06] px-3 py-2.5">
        <Search className="size-4 shrink-0 text-[#aebfff]" />
        <span className="min-w-0 flex-1 truncate text-[12px] text-white/55">{answer ? answer.q : d.recall.placeholder}</span>
      </div>
      <div className="mt-2.5 flex flex-wrap gap-1.5">
        {d.recall.chips.map((chip, i) => (
          <button
            key={chip}
            type="button"
            onClick={() => ask(i)}
            className={`min-h-8 rounded-full px-3 py-1.5 text-left text-[11px] font-medium transition-colors ${
              picked === i ? 'bg-[#5273df] text-white' : 'bg-white/8 text-white/72 hover:bg-white/16 hover:text-white'
            }`}
          >
            {chip}
          </button>
        ))}
      </div>
      <div className="mt-3 rounded-xl border border-white/8 bg-white/[0.04] p-3.5">
        {thinking && <p className="text-[12px] text-white/45">···</p>}
        {!thinking && !answer && <p className="text-[12px] leading-relaxed text-white/45">{d.recall.empty}</p>}
        {!thinking && answer && (
          <>
            <p className="text-[10px] font-semibold tracking-[0.1em] text-[#9db2ff]">{answer.time}</p>
            <p className="mt-1.5 text-[13px] leading-relaxed text-white/90">{answer.text}</p>
            <p className="mt-2.5 border-t border-white/8 pt-2.5 text-[10px] text-white/45">{answer.src}</p>
          </>
        )}
      </div>
    </div>
  );
}

function DraftPane({ d, after }: { d: Demo; after: (ms: number, fn: () => void) => void }) {
  const [shown, setShown] = useState(0);
  const [running, setRunning] = useState(false);
  const press = () => {
    if (running) return;
    setRunning(true);
    setShown(0);
    const step = (n: number) => {
      after(18 * n, () => {
        setShown(n);
        if (n >= d.draft.typed.length) setRunning(false);
      });
    };
    for (let n = 1; n <= d.draft.typed.length; n += 1) step(n);
  };

  return (
    <div>
      <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-[#9db2ff]">{d.draft.field}</p>
      <div className="mt-2 min-h-[128px] rounded-xl border border-white/12 bg-white/[0.06] p-3.5">
        {shown === 0 ? (
          <p className="text-[12px] text-white/40">{d.draft.placeholder}</p>
        ) : (
          <p className="text-[13px] leading-relaxed text-white/90">
            {d.draft.typed.slice(0, shown)}
            <span className="ml-0.5 inline-block h-[15px] w-px translate-y-[2px] bg-[#aebfff]" />
          </p>
        )}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={press}
          className="flex min-h-9 items-center gap-2 rounded-full bg-white px-3.5 text-[11px] font-bold text-[#10224d] transition-transform hover:-translate-y-px"
        >
          <Command className="size-3.5" />
          {d.draft.press}
        </button>
        <button
          type="button"
          onClick={() => {
            setRunning(false);
            setShown(0);
          }}
          className="min-h-9 rounded-full bg-white/8 px-3.5 text-[11px] font-semibold text-white/70 transition-colors hover:bg-white/16 hover:text-white"
        >
          {d.draft.reset}
        </button>
      </div>
      <p className="mt-3 text-[10px] leading-relaxed text-white/45">{d.draft.note}</p>
    </div>
  );
}

function MeetingPane({ d, after }: { d: Demo; after: (ms: number, fn: () => void) => void }) {
  const [phase, setPhase] = useState<'idle' | 'live' | 'minutes'>('idle');
  const [lines, setLines] = useState(0);

  const start = () => {
    setPhase('live');
    setLines(0);
    d.meeting.lines.forEach((_, i) => after(650 * (i + 1), () => setLines(i + 1)));
  };

  return (
    <div>
      {phase === 'idle' && (
        <div className="flex min-h-[168px] flex-col items-center justify-center rounded-xl border border-white/8 bg-white/[0.04] p-4 text-center">
          <button
            type="button"
            onClick={start}
            className="flex min-h-10 items-center gap-2 rounded-full bg-[#87e5b4] px-4 text-[12px] font-bold text-[#0a2b1c] transition-transform hover:-translate-y-px"
          >
            <Mic className="size-4" />
            {d.meeting.start}
          </button>
        </div>
      )}

      {phase === 'live' && (
        <div className="rounded-xl border border-white/8 bg-white/[0.04] p-3.5">
          <p className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.1em] text-[#87e5b4]">
            <span className="size-1.5 animate-pulse rounded-full bg-[#87e5b4]" /> {d.meeting.live}
          </p>
          <div className="mt-2.5 min-h-[96px] space-y-1.5">
            {d.meeting.lines.slice(0, lines).map((line) => (
              <p key={line} className="text-[12px] leading-relaxed text-white/85">
                {line}
              </p>
            ))}
          </div>
          <button
            type="button"
            onClick={() => setPhase('minutes')}
            className="mt-3 min-h-9 w-full rounded-full bg-white px-3.5 text-[11px] font-bold text-[#10224d]"
          >
            {d.meeting.stop}
          </button>
        </div>
      )}

      {phase === 'minutes' && (
        <div className="rounded-xl border border-white/8 bg-white/[0.04] p-3.5">
          <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-[#9db2ff]">{d.meeting.minutesTitle}</p>
          <p className="mt-2 text-[13px] leading-relaxed text-white/90">{d.meeting.decisions}</p>
          <p className="mt-3 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/45">{d.meeting.actionsTitle}</p>
          <ul className="mt-1.5 space-y-1.5">
            {d.meeting.actions.map((item) => (
              <li key={item} className="flex items-start gap-2 text-[12px] leading-relaxed text-white/85">
                <Check className="mt-0.5 size-3.5 shrink-0 text-[#87e5b4]" />
                {item}
              </li>
            ))}
          </ul>
          <button
            type="button"
            onClick={() => {
              setPhase('idle');
              setLines(0);
            }}
            className="mt-3 min-h-9 w-full rounded-full bg-white/8 px-3.5 text-[11px] font-semibold text-white/70 hover:bg-white/16 hover:text-white"
          >
            {d.meeting.reset}
          </button>
        </div>
      )}
      <p className="mt-3 text-[10px] leading-relaxed text-white/45">{d.meeting.note}</p>
    </div>
  );
}
