import { Workflow } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import { BrandIcon } from '@/components/BrandIcon';
import { Logo } from '@/components/Logo';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

const SOURCE_ITEMS = [
  { label: 'Slack', domain: 'slack.com' },
  { label: 'Gmail', domain: 'gmail.com' },
  { label: 'Notion', domain: 'notion.so' },
  { label: 'Calendar', domain: 'calendar.google.com' },
  { label: 'GitHub', domain: 'github.com' },
] as const;

const MODEL_ITEMS = [
  { label: 'Claude', domain: 'anthropic.com' },
  { label: 'Cursor', domain: 'cursor.com' },
  { label: 'ChatGPT', domain: 'openai.com' },
] as const;

type HowCopy = {
  connected: string;
  structuredProfile: string;
  sharedMemory: string;
  liveContext: string;
  contextReady: string;
  profileChips: readonly string[];
};

function SourcesVisual({ copy }: { copy: HowCopy }) {
  return (
    <div className="h-[306px] overflow-hidden rounded-[26px] bg-[radial-gradient(circle_at_50%_8%,rgba(255,255,255,0.14),transparent_30%),linear-gradient(180deg,#171717_0%,#1c1c1c_100%)] p-4">
      <div className="grid h-full grid-rows-5 gap-2">
        {SOURCE_ITEMS.map(({ label, domain }, index) => (
          <div
            key={label}
            className="flex min-h-0 items-center justify-between gap-3 rounded-[16px] border border-[#26452f] bg-[#1d2b22] px-3 shadow-[0_10px_22px_rgba(0,0,0,0.18)]"
            style={{ opacity: 1 - index * 0.075 }}
          >
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-[11px] border border-white/15 bg-white shadow-[inset_0_1px_0_rgba(255,255,255,0.72)]">
                <BrandIcon domain={domain} name={label} size={24} className="size-6" />
              </span>
              <span className="truncate text-[14px] font-semibold text-white">{label}</span>
            </div>
            <span className="shrink-0 rounded-full bg-[#183d25] px-2.5 py-1 text-[10px] font-semibold text-[#56dc88] sm:text-[11px]">
              {copy.connected}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function MemoryVisual({ copy }: { copy: HowCopy }) {
  return (
    <div className="flex h-[306px] items-center justify-center overflow-hidden rounded-[26px] bg-[radial-gradient(circle_at_50%_18%,rgba(255,221,150,0.14),transparent_30%),linear-gradient(180deg,#181614_0%,#1b1a18_100%)] p-5">
      <div className="w-full max-w-[286px] rounded-[26px] border border-[#8f6a21] bg-[linear-gradient(180deg,rgba(95,60,12,0.78),rgba(61,41,12,0.86))] p-4 shadow-[0_20px_44px_rgba(0,0,0,0.25)]">
        <div className="mx-auto w-fit max-w-full rounded-full border border-[#ae8532] bg-[#3a2a12] px-3 py-1 text-center text-[10px] font-semibold leading-tight text-[#ffd98b]">
          {copy.structuredProfile}
        </div>
        <div className="mt-4 flex items-center justify-center gap-2.5 text-[#fff2cf]">
          <Workflow className="size-6 shrink-0" strokeWidth={2} />
          <span className="text-center text-[17px] font-semibold tracking-[-0.02em]">{copy.sharedMemory}</span>
        </div>
        <div className="mt-4 grid grid-cols-2 gap-2">
          {copy.profileChips.map((chip) => (
            <span key={chip} className="flex min-h-9 items-center justify-center rounded-[13px] border border-[#8f6a21] bg-[#2f2415] px-2 py-1.5 text-center text-[10px] font-medium leading-tight text-[#ead6aa]">
              {chip}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

function ProvidersVisual({ copy }: { copy: HowCopy }) {
  return (
    <div className="flex h-[306px] flex-col overflow-hidden rounded-[26px] bg-[radial-gradient(circle_at_50%_12%,rgba(255,255,255,0.16),transparent_30%),linear-gradient(180deg,#171717_0%,#1b1b1b_100%)] p-4">
      <div className="flex min-h-[58px] items-center justify-between gap-3 rounded-[18px] border border-[#836029] bg-[#362816] px-3.5 py-2.5 text-[#f0d39b] shadow-[0_16px_30px_rgba(0,0,0,0.2)]">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-[11px] bg-[#ead49a]">
            <Logo size={20} />
          </span>
          <span className="truncate text-[13px] font-semibold sm:text-[14px]">ShogunAI Memory</span>
        </div>
        <span className="shrink-0 text-[10px] text-[#d6b97d]">{copy.liveContext}</span>
      </div>

      <div className="mt-auto grid grid-cols-3 gap-2">
        {MODEL_ITEMS.map((item) => (
          <div key={item.label} className="flex min-w-0 flex-col items-center rounded-[17px] border border-[#29452f] bg-[#16261c] px-1.5 py-3 text-center shadow-[0_14px_26px_rgba(0,0,0,0.18)]">
            <span className="mb-2 flex size-10 items-center justify-center rounded-[12px] border border-white/10 bg-white">
              <BrandIcon domain={item.domain} name={item.label} size={26} className="size-[26px]" />
            </span>
            <span className="w-full truncate text-[12px] font-semibold text-white sm:text-[13px]">{item.label}</span>
            <span className="mt-1 min-h-[30px] text-[9px] font-medium leading-[1.45] text-[#60da88] sm:text-[10px]">
              {copy.contextReady}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function VisualCard({ index, copy }: { index: number; copy: HowCopy }) {
  if (index === 0) return <SourcesVisual copy={copy} />;
  if (index === 1) return <MemoryVisual copy={copy} />;
  return <ProvidersVisual copy={copy} />;
}

const COPY: Record<Locale, HowCopy> = {
  en: {
    connected: 'Connected',
    structuredProfile: 'Structured profile',
    sharedMemory: 'Shared memory',
    liveContext: 'Live context',
    contextReady: 'Context ready',
    profileChips: ['Role: Founder', 'Focus: Growth', 'Stack: MCP', 'Project: Launch'],
  },
  ja: {
    connected: '接続済み',
    structuredProfile: '構造化プロフィール',
    sharedMemory: '共有メモリ',
    liveContext: 'ライブ文脈',
    contextReady: '文脈準備済み',
    profileChips: ['役割：創業者', '注力：成長', '技術：MCP', '案件：公開'],
  },
  es: {
    connected: 'Conectado',
    structuredProfile: 'Perfil estructurado',
    sharedMemory: 'Memoria compartida',
    liveContext: 'Contexto activo',
    contextReady: 'Contexto listo',
    profileChips: ['Rol: fundador', 'Foco: crecimiento', 'Stack: MCP', 'Proyecto: lanzamiento'],
  },
  de: {
    connected: 'Verbunden',
    structuredProfile: 'Strukturiertes Profil',
    sharedMemory: 'Gemeinsames Gedächtnis',
    liveContext: 'Live-Kontext',
    contextReady: 'Kontext bereit',
    profileChips: ['Rolle: Gründer', 'Fokus: Wachstum', 'Stack: MCP', 'Projekt: Launch'],
  },
};

export function How({ t, locale }: { t: Dictionary; locale: Locale }) {
  const copy = COPY[locale];

  return (
    <section id="how" className="scroll-mt-20 py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <Reveal className="mx-auto mb-12 max-w-[840px] text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.how.eyebrow}</p>
          <h2 className="how-title mt-3.5 font-display text-[clamp(24px,5.5vw,44px)] font-semibold leading-[1.1] tracking-[-0.015em] text-balance">
            {t.how.title}
          </h2>
        </Reveal>

        <div className="grid gap-5 xl:grid-cols-3">
          {t.how.steps.map((step, index) => (
            <Reveal key={step.title} delay={index * 0.08}>
              <div className="theme-dark-panel lift flex h-full flex-col overflow-hidden rounded-[30px] border border-white/10 bg-[linear-gradient(180deg,#141414_0%,#101010_100%)] p-4 shadow-[0_24px_70px_rgba(0,0,0,0.14)]">
                <VisualCard index={index} copy={copy} />
                <div className="flex flex-1 flex-col px-2 pb-3 pt-5">
                  <div className="mb-3 font-mono text-sm text-[#d9b56d]">{String(index + 1).padStart(2, '0')}</div>
                  <h3 className={`font-display text-[clamp(23px,2vw,30px)] font-semibold leading-[1.22] tracking-[-0.03em] text-white ${locale === 'ja' ? 'break-keep' : 'text-balance'}`}>
                    {step.title}
                  </h3>
                  <p className="how-step-body mt-3 text-[15px] leading-[1.8] text-white/66">{step.body}</p>
                </div>
              </div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
