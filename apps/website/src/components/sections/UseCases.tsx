import { ArrowRight, Lock, Unplug } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { BrandIcon } from '@/components/BrandIcon';
import { Logo } from '@/components/Logo';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

type Item = Dictionary['usecases']['items'][number];

const TOOL_CHIPS = [
  { label: 'ChatGPT', domain: 'openai.com', tone: 'from-[#dff8ef] to-white text-[#0d6b4a]' },
  { label: 'Claude', domain: 'anthropic.com', tone: 'from-[#f6eadf] to-white text-[#7a5a34]' },
  { label: 'Cursor', domain: 'cursor.com', tone: 'from-[#e8ecff] to-white text-[#4a62c6]' },
  { label: 'Slack', domain: 'slack.com', tone: 'from-[#efe5ff] to-white text-[#6d3dc4]' },
  { label: 'Gmail', domain: 'gmail.com', tone: 'from-[#ffe9e4] to-white text-[#d14d35]' },
  { label: 'Notion', domain: 'notion.so', tone: 'from-[#f0f2f4] to-white text-[#1f2730]' },
  { label: 'Calendar', domain: 'calendar.google.com', tone: 'from-[#e3edff] to-white text-[#3564cf]' },
  { label: 'GitHub', domain: 'github.com', tone: 'from-[#eef1f6] to-white text-[#283447]' },
] as const;

function ToolPill({
  label,
  className = '',
  tone = 'from-white to-white text-[#213547]',
}: {
  label: string;
  className?: string;
  tone?: string;
}) {
  return (
    <span
      className={`relative z-10 inline-flex items-center gap-1.5 rounded-[18px] border border-white/70 bg-gradient-to-b px-3 py-2 text-[12px] font-semibold shadow-[0_10px_24px_rgba(20,43,71,0.10)] backdrop-blur ${tone} ${className}`}
    >
      {(() => {
        const domain = TOOL_CHIPS.find((tool) => tool.label === label)?.domain;
        return domain ? <BrandIcon domain={domain} name={label} size={16} className="size-4" /> : null;
      })()}
      {label}
    </span>
  );
}

function GapIllustration({ index, labels }: { index: number; labels: { notes: string; history: string; context: string } }) {
  if (index === 0) {
    return (
      <div className="relative h-[230px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_22%,rgba(255,255,255,0.95),rgba(237,243,255,0.9)_36%,rgba(228,236,247,0.92)_68%,rgba(220,231,244,0.94)_100%)]">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_50%_100%,rgba(255,255,255,0.48),transparent_50%)]" />
        <svg className="absolute inset-[18%_10%_18%_10%] h-auto w-[80%]" viewBox="0 0 360 170" fill="none">
          <path d="M40 115C90 48 155 30 230 48C278 60 309 83 324 113" stroke="#bbc7d8" strokeWidth="3" strokeDasharray="10 12" strokeLinecap="round" />
          <path d="M53 82C112 106 180 116 245 104C276 98 301 86 323 63" stroke="#c7d2e1" strokeWidth="3" strokeDasharray="10 12" strokeLinecap="round" />
        </svg>
        <div className="absolute left-[8%] top-[46%] -translate-y-1/2">
          <ToolPill label="Cursor" tone={TOOL_CHIPS[2].tone} className="rotate-[-12deg]" />
        </div>
        <div className="absolute left-[42%] top-[18%] -translate-x-1/2">
          <ToolPill label="ChatGPT" tone={TOOL_CHIPS[0].tone} className="rotate-[8deg]" />
        </div>
        <div className="absolute right-[10%] top-[34%] z-10">
          <ToolPill label="Claude" tone={TOOL_CHIPS[1].tone} className="rotate-[11deg]" />
        </div>
        <div className="absolute right-[22%] bottom-[18%] z-10">
          <ToolPill label="Cursor" tone={TOOL_CHIPS[2].tone} className="rotate-[-9deg]" />
        </div>
        {[
          'left-[23%] top-[50%]',
          'left-[44%] top-[55%]',
          'left-[66%] top-[48%]',
          'left-[58%] top-[28%]',
        ].map((pos) => (
          <span
            key={pos}
            className={`absolute ${pos} flex size-8 items-center justify-center rounded-full bg-white/72 text-[#94a3b8] shadow-[0_10px_18px_rgba(24,42,67,0.08)]`}
          >
            <Unplug className="size-4" strokeWidth={2.4} />
          </span>
        ))}
      </div>
    );
  }

  if (index === 1) {
    return (
      <div className="relative h-[230px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_14%,rgba(255,255,255,0.96),rgba(233,241,255,0.92)_35%,rgba(223,234,250,0.94)_70%,rgba(218,230,247,0.96)_100%)]">
        <div className="absolute left-1/2 top-[44%] h-[124px] w-[212px] -translate-x-1/2 rounded-[32px] border border-white/70 bg-[linear-gradient(180deg,rgba(116,164,255,0.68),rgba(83,132,235,0.86))] shadow-[0_24px_50px_rgba(72,110,190,0.24)] backdrop-blur" />
        <div className="absolute left-1/2 top-[47%] z-0 flex h-[112px] w-[212px] -translate-x-1/2 items-center justify-center">
          <Lock className="size-8 text-white/88" strokeWidth={2.1} />
        </div>
        <div className="absolute inset-x-[14%] top-[28%] h-[90px] rounded-[999px] border border-white/55 bg-white/20 blur-[0.2px]" />
        <div className="absolute left-[12%] top-[26%]">
          <ToolPill label="Slack" tone={TOOL_CHIPS[3].tone} />
        </div>
        <div className="absolute left-[29%] top-[18%]">
          <ToolPill label="Gmail" tone={TOOL_CHIPS[4].tone} />
        </div>
        <div className="absolute right-[31%] top-[18%]">
          <ToolPill label="Notion" tone={TOOL_CHIPS[5].tone} />
        </div>
        <div className="absolute right-[12%] top-[26%]">
          <ToolPill label="Calendar" tone={TOOL_CHIPS[6].tone} />
        </div>
        <div className="absolute left-[22%] bottom-[26%] z-20 rounded-[14px] border border-white/45 bg-white/28 px-4 py-3 text-[11px] text-[#60738e] blur-[0.2px]">
          {labels.notes}
        </div>
        <div className="absolute left-[6%] bottom-[7%] z-20 rounded-[14px] border border-white/55 bg-white/72 px-4 py-3 text-[11px] font-medium text-[#516784]">
          {labels.history}
        </div>
        <div className="absolute right-[20%] bottom-[24%] z-20 rounded-[14px] border border-white/45 bg-white/28 px-4 py-3 text-[11px] text-[#60738e] blur-[0.2px]">
          {labels.context}
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-[230px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_12%,rgba(255,255,255,0.98),rgba(243,238,255,0.9)_32%,rgba(233,227,255,0.92)_68%,rgba(228,219,255,0.96)_100%)]">
      <div className="absolute left-1/2 top-[52%] h-[124px] w-[124px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-[radial-gradient(circle_at_34%_30%,rgba(255,255,255,0.95),rgba(198,179,255,0.92)_35%,rgba(166,141,255,0.98)_75%,rgba(140,115,246,1)_100%)] shadow-[0_20px_44px_rgba(132,108,233,0.24)]" />
      <div className="absolute left-1/2 top-[52%] -translate-x-1/2 -translate-y-1/2">
        <Logo size={34} className="drop-shadow-[0_6px_18px_rgba(88,72,180,0.25)]" />
      </div>
      <div className="absolute left-1/2 top-[52%] h-[154px] w-[154px] -translate-x-1/2 -translate-y-1/2 rounded-full border border-[#a98cff]/32" />
      <div className="absolute left-1/2 top-[52%] h-[198px] w-[86%] -translate-x-1/2 -translate-y-1/2 rounded-[999px] border border-[#ab92ff]/25" />
      <svg className="absolute inset-[12%_8%_12%_8%] h-auto w-[84%]" viewBox="0 0 420 220" fill="none">
        <ellipse cx="210" cy="112" rx="170" ry="72" stroke="#a78bfa" strokeOpacity="0.5" strokeWidth="3" />
        <path d="M40 112H380" stroke="#c8bbff" strokeOpacity="0.24" strokeWidth="2" strokeDasharray="6 10" />
      </svg>
      {[
        { label: 'ChatGPT', tone: TOOL_CHIPS[0].tone, cls: 'left-[5%] top-[52%] -translate-y-1/2' },
        { label: 'Slack', tone: TOOL_CHIPS[3].tone, cls: 'left-[22%] top-[22%]' },
        { label: 'Gmail', tone: TOOL_CHIPS[4].tone, cls: 'left-[68%] top-[22%]' },
        { label: 'Notion', tone: TOOL_CHIPS[5].tone, cls: 'right-[6%] top-[42%]' },
        { label: 'Calendar', tone: TOOL_CHIPS[6].tone, cls: 'right-[11%] bottom-[18%]' },
        { label: 'GitHub', tone: TOOL_CHIPS[7].tone, cls: 'left-[64%] bottom-[16%]' },
        { label: 'Cursor', tone: TOOL_CHIPS[2].tone, cls: 'left-[16%] bottom-[20%]' },
      ].map((node) => (
        <div key={node.label + node.cls} className={`absolute ${node.cls}`}>
          <ToolPill label={node.label} tone={node.tone} />
        </div>
      ))}
      {['left-[28%] top-[35%]', 'left-[49%] top-[20%]', 'left-[76%] top-[34%]', 'left-[78%] bottom-[30%]', 'left-[36%] bottom-[20%]'].map(
        (pos) => (
          <span key={pos} className={`absolute ${pos} size-3 rounded-full bg-[#a78bfa] shadow-[0_0_0_6px_rgba(167,139,250,0.12)]`} />
        ),
      )}
    </div>
  );
}

function DemoCard({ item, i, labels }: { item: Item; i: number; labels: { notes: string; history: string; context: string } }) {
  return (
    <Reveal delay={i * 0.08} y={26}>
      <figure className="theme-surface-card lift flex h-full flex-col rounded-[28px] border border-border/85 bg-[linear-gradient(180deg,rgba(255,255,255,0.98),rgba(246,250,255,0.98))] p-4 shadow-[0_24px_70px_rgba(14,30,56,0.08)] hover:border-accent/30 sm:p-5">
        <div className="mb-4 flex items-center justify-between gap-3">
          <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-[#2563eb]">{item.persona}</span>
          <span className="rounded-full border border-border/70 bg-white/72 px-3 py-1 text-[11px] font-medium text-muted shadow-[0_4px_12px_rgba(14,30,56,0.05)]">
            {item.chip}
          </span>
        </div>
        <GapIllustration index={i} labels={labels} />
        <div className="mt-5 px-1">
          <h3 className="font-display text-[21px] font-semibold leading-[1.25] tracking-[-0.02em] text-ink">{item.title}</h3>
          <p className="mt-2.5 text-[14px] leading-[1.8] text-muted">{item.body}</p>
        </div>
      </figure>
    </Reveal>
  );
}

export function UseCases({ t, locale }: { t: Dictionary; locale: Locale }) {
  const explore = { en: 'Explore role-specific workflows', ja: '職種別の活用方法を見る', es: 'Explorar flujos por perfil', de: 'Rollenspezifische Workflows ansehen' }[locale];
  const labels = locale === 'ja'
    ? { notes: 'メモ', history: '履歴', context: '文脈' }
    : locale === 'es'
      ? { notes: 'notas', history: 'historial', context: 'contexto' }
      : locale === 'de'
        ? { notes: 'Notizen', history: 'Verlauf', context: 'Kontext' }
        : { notes: 'notes', history: 'history', context: 'context' };
  return (
    <section id="usecases" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[48ch] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.usecases.eyebrow}</p>
          <h2 className="usecases-title mt-3.5 font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.usecases.title}
          </h2>
          <p className="usecases-sub mt-4 text-[15px] leading-relaxed text-muted">{t.usecases.sub}</p>
        </Reveal>

        <div className="grid gap-5 xl:grid-cols-3">
          {t.usecases.items.map((item, i) => (
            <DemoCard key={item.title} item={item} i={i} labels={labels} />
          ))}
        </div>
        <div className="mt-8 text-center">
          <a href={`/${locale}/use-cases`} className="inline-flex items-center gap-2 text-sm font-semibold text-accent hover:text-accent-strong">{explore} <ArrowRight className="size-4" /></a>
        </div>
      </div>
    </section>
  );
}
