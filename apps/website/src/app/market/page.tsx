import type { Metadata } from 'next';
import { ArrowUpRight } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Market — What investors are saying about the memory & context layer',
  description:
    'Sourced public commentary from a16z, Emergence Capital, Kindred Ventures, Wing and others on memory, context and agents as coworkers — and where ShogunAI fits.',
  alternates: { canonical: '/en/market', languages: localizedAlternates('/market') },
};

/** Shared, locale-independent attribution for each quote card. */
const QUOTES = [
  {
    firm: 'Andreessen Horowitz',
    mark: 'a16z',
    name: 'Jason Cui & Jennifer Li',
    source: 'Your Data Agents Need Context',
    href: 'https://a16z.com/your-data-agents-need-context/',
  },
  {
    firm: 'Emergence Capital',
    mark: 'EC',
    name: 'Gordon Ritter',
    source: 'AI Coaching Networks',
    href: 'https://www.emcap.com/conviction-areas/ai-coaching-networks',
  },
  {
    firm: 'Kindred Ventures',
    mark: 'KV',
    name: 'Kindred Ventures',
    source: 'Building the Memory Infrastructure for Personalized AI',
    href: 'https://kindredventures.com/announcement/mem0-building-the-memory-infrastructure-for-personalized-ai/',
  },
] as const;

/** Card surfaces mirror the reference layout: saturated fill, dark ink, no theme flip. */
const CARD_SKINS = [
  { bg: 'linear-gradient(158deg,#ffc0a6 0%,#ff9370 46%,#f4663d 100%)', ink: '#2c1105', chip: 'rgba(255,255,255,0.58)' },
  { bg: 'linear-gradient(158deg,#c9f095 0%,#96de62 46%,#63ba3a 100%)', ink: '#14290a', chip: 'rgba(255,255,255,0.58)' },
  { bg: 'linear-gradient(158deg,#e9df94 0%,#d2c661 46%,#ab9e36 100%)', ink: '#251f05', chip: 'rgba(255,255,255,0.58)' },
] as const;

const SOURCES = [
  ['Your Data Agents Need Context — a16z', 'https://a16z.com/your-data-agents-need-context/'],
  ['AI Coaching Networks — Emergence Capital', 'https://www.emcap.com/conviction-areas/ai-coaching-networks'],
  ['Genspark: The Ultimate System of Context — Emergence', 'https://www.emcap.com/thoughts/genspark-the-ultimate-system-of-context'],
  ['Mem0 raises $24M Series A — PR Newswire', 'https://www.prnewswire.com/news-releases/mem0-raises-24m-series-a-to-build-memory-layer-for-ai-agents-302597157.html'],
  ['Mem0: Building the Memory Infrastructure for Personalized AI — Kindred Ventures', 'https://kindredventures.com/announcement/mem0-building-the-memory-infrastructure-for-personalized-ai/'],
  ['The Emerging Agentic Enterprise — MIT SMR / BCG', 'https://sloanreview.mit.edu/projects/the-emerging-agentic-enterprise-how-leaders-must-navigate-a-new-age-of-ai/'],
  ['The Rise of the Agentic Workforce — Wing VC', 'https://www.wing.vc/content/the-rise-of-the-agentic-workforce'],
] as const;

const content = {
  en: {
    eyebrow: 'Market',
    title: 'Memory and context are becoming their own market.',
    lead: [
      'For most of the last two years the bottleneck was the model. It isn’t any more. The funds backing agent infrastructure have converged on a different constraint: an agent is only as useful as the context it can reach, and almost nobody’s context is reachable.',
      'What follows is public commentary about that category — not an endorsement of ShogunAI. We collect it because outside observers keep describing, in their own words, the two halves of the thing we are building: memory that captures your day, and execution that acts on it.',
    ],
    quotesLabel: 'What investors are saying about the category',
    quotes: [
      '“A new category of company has emerged that is building context layers from the ground up.”',
      '“The future of software is driven by human brilliance, with AI in the back seat.”',
      '“Memory is the engine that makes personalization possible.”',
    ],
    signalsLabel: 'Signals',
    signals: [
      ['$24M', 'Mem0’s Series A', 'Raised to build a dedicated memory layer for AI agents — led by Basis Set, with Kindred, Peak XV, GitHub Fund and Y Combinator.'],
      ['$275M', 'Genspark’s Series B', 'Emergence framed it as a move from model-centric tools to outcome-centric systems built on “an architecture that captures and applies context at every step.”'],
      ['76%', 'see agents as coworkers', 'Of executives surveyed by MIT Sloan Management Review and BCG. It measures perception, not agent capability.'],
      ['10×', 'the budget in play', 'Wing’s Tanay Jaipuria argues agents go after labor budgets rather than software budgets — “at least an order of magnitude larger than software spend.”'],
    ],
    thesisLabel: 'Where we sit',
    thesisTitle: 'Two theses, one product',
    thesisBody: 'Read together, the commentary above splits cleanly in two. One half says memory and context are the new infrastructure layer. The other says agents only become valuable when they behave like a coworker — which is impossible without that layer. They are the same claim from two directions.',
    theses: [
      ['Memory and context are infrastructure.', 'Not a feature bolted onto a chat window — a durable layer that outlives whichever model you happen to be using this quarter. ShogunAI keeps that layer on your Mac, so switching providers costs you nothing.'],
      ['Acting on context is the actual product.', 'A memory that only answers questions is a well-behaved search box. The reason to know what happened in your day is to close what it left open — inside the tools you already use, with approval before anything leaves your machine.'],
    ],
    readMore: 'Read the full sourced write-up',
    notesLabel: 'On sourcing',
    notesBody: 'Every quote on this page is taken from a public post or press release by the firm named, and each card links to its source. These are investor theses about where the market is going — opinions about the future, not settled facts — and none of them are statements about ShogunAI.',
    translationNote: '',
  },
  ja: {
    eyebrow: '市場',
    title: 'メモリと文脈は、ひとつの市場になりつつある。',
    lead: [
      'この2年ほど、ボトルネックはモデルでした。もうそうではありません。エージェント基盤に出資するファンドの視線は、別の制約に集まっています ── エージェントの有用性は、届く文脈の広さで決まる。そして、ほとんどの人の文脈はどこにも届いていない。',
      '以下は、そのカテゴリについて公開されている第三者の発言であり、ShogunAI への推薦ではありません。それでもここに並べているのは、外から見た人たちが、私たちの作っているものの両輪 ── 一日を記憶するメモリと、それに基づいて動く実行 ── を、それぞれの言葉で説明し続けているからです。',
    ],
    quotesLabel: '投資家がこのカテゴリについて語っていること',
    quotes: [
      '「文脈レイヤーをゼロから作る、新しいカテゴリの企業が現れている。」',
      '「ソフトウェアの未来を動かすのは人間の才気であり、AI は後部座席にいる。」',
      '「パーソナライゼーションを可能にするエンジン、それがメモリだ。」',
    ],
    signalsLabel: 'シグナル',
    signals: [
      ['$24M', 'Mem0 のシリーズA', 'AIエージェント向けの専用メモリレイヤーを作るための調達。Basis Set がリードし、Kindred、Peak XV、GitHub Fund、Y Combinator が参加。'],
      ['$275M', 'Genspark のシリーズB', 'Emergence はこれを、モデル中心のツールから成果中心のシステムへの移行と位置づけ、「あらゆる段階で文脈を取得し適用するアーキテクチャ」と表現しました。'],
      ['76%', 'エージェントを「同僚」と見る', 'MIT Sloan Management Review と BCG による経営層調査より。測っているのは認識であって、エージェントの能力ではありません。'],
      ['10×', '狙っている予算の大きさ', 'Wing の Tanay Jaipuria は、エージェントはソフトウェア予算ではなく人件費の予算を取りにいくため、機会は「ソフトウェア支出より少なくとも一桁大きい」と論じています。'],
    ],
    thesisLabel: '私たちの立ち位置',
    thesisTitle: 'ふたつの仮説、ひとつのプロダクト',
    thesisBody: '並べて読むと、上の発言はきれいに二分されます。片方は「メモリと文脈が新しい基盤レイヤーだ」と言い、もう片方は「エージェントは同僚のように振る舞ってはじめて価値になる」と言う。後者は、そのレイヤーなしには成立しません。方向が違うだけで、同じ主張です。',
    theses: [
      ['メモリと文脈は、インフラである。', 'チャット画面に後付けする機能ではなく、今期たまたま使っているモデルより長く残るレイヤーです。ShogunAI はそれをあなたの Mac に置きます。だから提供者を乗り換えても、失うものはありません。'],
      ['文脈に基づいて動くことが、製品の本体。', '質問に答えるだけのメモリは、行儀のいい検索ボックスです。一日に何があったかを知る理由は、そこで開いたままのものを片付けるため ── あなたがすでに使っているツールの中で、端末を出るものは必ずあなたの承認を待たせて。'],
    ],
    readMore: '出典つきの詳しい記事を読む',
    notesLabel: '出典について',
    notesBody: 'このページの引用はすべて、記載した各社の公開記事やプレスリリースからのもので、各カードから出典に移動できます。いずれも市場の行き先についての投資家の仮説 ── 将来についての意見であって、確定した事実ではありません ── であり、ShogunAI について述べたものではありません。',
    translationNote: '引用は英語の原文を訳したものです。原文は各カードのリンク先で確認できます。',
  },
  es: {
    eyebrow: 'Mercado',
    title: 'La memoria y el contexto se están convirtiendo en un mercado propio.',
    lead: [
      'Durante casi dos años el cuello de botella fue el modelo. Ya no lo es. Los fondos que financian la infraestructura de agentes han convergido en otra restricción: un agente sirve tanto como el contexto que alcanza, y casi nadie tiene su contexto al alcance.',
      'Lo que sigue son comentarios públicos sobre esa categoría, no un respaldo a ShogunAI. Los recogemos porque observadores externos describen, con sus propias palabras, las dos mitades de lo que estamos construyendo: memoria que captura tu día y ejecución que actúa sobre ella.',
    ],
    quotesLabel: 'Lo que dicen los inversores sobre la categoría',
    quotes: [
      '«Ha surgido una nueva categoría de empresas que construye capas de contexto desde cero.»',
      '«El futuro del software lo impulsa el talento humano, con la IA en el asiento trasero.»',
      '«La memoria es el motor que hace posible la personalización.»',
    ],
    signalsLabel: 'Señales',
    signals: [
      ['$24M', 'Serie A de Mem0', 'Levantada para construir una capa de memoria dedicada a agentes de IA — liderada por Basis Set, con Kindred, Peak XV, GitHub Fund e Y Combinator.'],
      ['$275M', 'Serie B de Genspark', 'Emergence lo presentó como el paso de herramientas centradas en el modelo a sistemas centrados en el resultado, sobre «una arquitectura que captura y aplica contexto en cada paso».'],
      ['76%', 've a los agentes como colegas', 'De los directivos encuestados por MIT Sloan Management Review y BCG. Mide percepción, no capacidad del agente.'],
      ['10×', 'el presupuesto en juego', 'Tanay Jaipuria, de Wing, sostiene que los agentes van a por presupuestos de personal y no de software: «al menos un orden de magnitud mayor que el gasto en software».'],
    ],
    thesisLabel: 'Dónde encajamos',
    thesisTitle: 'Dos tesis, un producto',
    thesisBody: 'Leídos juntos, los comentarios anteriores se dividen en dos. Una mitad dice que la memoria y el contexto son la nueva capa de infraestructura. La otra dice que los agentes solo valen cuando se comportan como un colega, algo imposible sin esa capa. Son la misma afirmación desde dos direcciones.',
    theses: [
      ['La memoria y el contexto son infraestructura.', 'No una función añadida a una ventana de chat, sino una capa duradera que sobrevive al modelo que uses este trimestre. ShogunAI la mantiene en tu Mac, así que cambiar de proveedor no te cuesta nada.'],
      ['Actuar sobre el contexto es el producto.', 'Una memoria que solo responde preguntas es un buscador educado. Saber qué pasó en tu día sirve para cerrar lo que quedó abierto, dentro de las herramientas que ya usas y con aprobación antes de que algo salga de tu equipo.'],
    ],
    readMore: 'Leer el análisis completo con fuentes',
    notesLabel: 'Sobre las fuentes',
    notesBody: 'Cada cita procede de una publicación o nota de prensa pública de la firma citada, y cada tarjeta enlaza a su fuente. Son tesis de inversores sobre hacia dónde va el mercado — opiniones sobre el futuro, no hechos establecidos — y ninguna se refiere a ShogunAI.',
    translationNote: 'Las citas están traducidas del inglés original; el enlace de cada tarjeta lleva al texto original.',
  },
  de: {
    eyebrow: 'Markt',
    title: 'Memory und Kontext werden zu einem eigenen Markt.',
    lead: [
      'Fast zwei Jahre lang war das Modell der Engpass. Das ist vorbei. Die Fonds, die Agenten-Infrastruktur finanzieren, sind sich über eine andere Grenze einig: Ein Agent ist nur so nützlich wie der Kontext, den er erreicht — und kaum jemandes Kontext ist erreichbar.',
      'Was folgt, sind öffentliche Aussagen über diese Kategorie, keine Empfehlung für ShogunAI. Wir sammeln sie, weil Außenstehende immer wieder in eigenen Worten die zwei Hälften dessen beschreiben, was wir bauen: ein Gedächtnis, das deinen Tag erfasst, und eine Ausführung, die darauf handelt.',
    ],
    quotesLabel: 'Was Investoren über die Kategorie sagen',
    quotes: [
      '„Eine neue Kategorie von Unternehmen ist entstanden, die Kontextebenen von Grund auf baut."',
      '„Die Zukunft der Software wird von menschlicher Brillanz getrieben, mit der KI auf dem Rücksitz."',
      '„Memory ist der Motor, der Personalisierung möglich macht."',
    ],
    signalsLabel: 'Signale',
    signals: [
      ['$24M', 'Series A von Mem0', 'Für eine dedizierte Memory-Ebene für KI-Agenten — angeführt von Basis Set, mit Kindred, Peak XV, GitHub Fund und Y Combinator.'],
      ['$275M', 'Series B von Genspark', 'Emergence beschrieb den Schritt von modellzentrierten Werkzeugen zu ergebniszentrierten Systemen auf „einer Architektur, die Kontext in jedem Schritt erfasst und anwendet".'],
      ['76%', 'sehen Agenten als Kollegen', 'Der von MIT Sloan Management Review und BCG befragten Führungskräfte. Gemessen wird Wahrnehmung, nicht Fähigkeit.'],
      ['10×', 'das Budget, um das es geht', 'Tanay Jaipuria von Wing argumentiert, Agenten zielen auf Personal- statt Softwarebudgets — „mindestens eine Größenordnung größer als Softwareausgaben".'],
    ],
    thesisLabel: 'Wo wir stehen',
    thesisTitle: 'Zwei Thesen, ein Produkt',
    thesisBody: 'Zusammengelesen zerfallen die Aussagen oben sauber in zwei Teile. Die eine Hälfte sagt: Memory und Kontext sind die neue Infrastrukturebene. Die andere sagt: Agenten werden erst wertvoll, wenn sie sich wie Kollegen verhalten — und das geht ohne diese Ebene nicht. Es ist dieselbe Behauptung aus zwei Richtungen.',
    theses: [
      ['Memory und Kontext sind Infrastruktur.', 'Keine Funktion, die an ein Chatfenster geschraubt wird, sondern eine Ebene, die das Modell überdauert, das du gerade zufällig nutzt. ShogunAI hält sie auf deinem Mac — ein Anbieterwechsel kostet dich nichts.'],
      ['Auf Kontext zu handeln ist das eigentliche Produkt.', 'Ein Gedächtnis, das nur Fragen beantwortet, ist ein wohlerzogenes Suchfeld. Zu wissen, was an deinem Tag passiert ist, dient dazu, das Offene zu schließen — in deinen Werkzeugen, mit Freigabe, bevor etwas dein Gerät verlässt.'],
    ],
    readMore: 'Die vollständige Analyse mit Quellen lesen',
    notesLabel: 'Zu den Quellen',
    notesBody: 'Jedes Zitat stammt aus einem öffentlichen Beitrag oder einer Pressemitteilung der genannten Firma; jede Karte verlinkt ihre Quelle. Es sind Investorenthesen über die Richtung des Marktes — Meinungen über die Zukunft, keine gesicherten Fakten — und keine davon handelt von ShogunAI.',
    translationNote: 'Die Zitate sind aus dem englischen Original übersetzt; der Link jeder Karte führt zum Originaltext.',
  },
} as const;

export default async function MarketPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const c = content[locale];
  const prefix = `/${locale}`;

  return (
    <PageShell locale={locale}>
      <JsonLd
        data={breadcrumbSchema([
          { name: 'Home', url: `${siteConfig.url}${prefix}` },
          { name: c.eyebrow, url: `${siteConfig.url}${prefix}/market` },
        ])}
      />

      {/* Headline + two columns of lead copy */}
      <header className="border-b border-border bg-[radial-gradient(120%_110%_at_18%_-40%,var(--color-sky-soft)_0%,transparent_62%)]">
        <div className="container-x py-[clamp(40px,7.5vw,100px)]">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{c.eyebrow}</p>
          <h1 className="mt-4 max-w-[1100px] font-display text-[clamp(32px,5.4vw,64px)] font-semibold leading-[1.04] tracking-[-0.03em] text-balance">
            {c.title}
          </h1>
          <div className="mt-8 grid gap-6 md:grid-cols-2 md:gap-12 lg:max-w-[85%]">
            {c.lead.map((paragraph) => (
              <p key={paragraph.slice(0, 24)} className="text-[15px] leading-[1.75] text-muted">
                {paragraph}
              </p>
            ))}
          </div>
        </div>
      </header>

      {/* Quote cards */}
      <section className="py-[clamp(44px,7vw,88px)]">
        <div className="container-x">
          <div className="flex items-center gap-5">
            <p className="shrink-0 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted">{c.quotesLabel}</p>
            <div className="h-px flex-1 bg-border" />
          </div>
          <div className="mt-8 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
            {QUOTES.map((q, index) => {
              const skin = CARD_SKINS[index];
              return (
                <a
                  key={q.href}
                  href={q.href}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{ background: skin.bg, color: skin.ink }}
                  className="group flex min-h-[360px] min-w-0 flex-col justify-between rounded-[30px] p-7 transition-transform duration-300 hover:-translate-y-1 sm:p-8"
                >
                  <div className="flex items-start justify-between gap-4">
                    <span className="font-display text-[15px] font-semibold tracking-tight">{q.firm}</span>
                    <ArrowUpRight className="size-5 shrink-0 opacity-55 transition-opacity group-hover:opacity-100" />
                  </div>
                  <blockquote className="mt-10 font-display text-[clamp(20px,1.7vw,25px)] font-medium leading-[1.28] tracking-[-0.01em]">
                    {c.quotes[index]}
                  </blockquote>
                  <div className="mt-8 flex items-center gap-3">
                    <span
                      style={{ background: skin.chip }}
                      className="flex size-11 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold tracking-tight"
                    >
                      {q.mark}
                    </span>
                    <span className="block min-w-0 flex-1">
                      <span className="block truncate text-sm font-semibold">{q.name}</span>
                      <span className="block truncate text-xs opacity-70">{q.source}</span>
                    </span>
                  </div>
                </a>
              );
            })}
          </div>
          {c.translationNote && <p className="mt-5 text-xs text-faint">{c.translationNote}</p>}
        </div>
      </section>

      {/* Funding + survey signals */}
      <section className="border-y border-border bg-cloud/45 py-[clamp(44px,7vw,88px)]">
        <div className="container-x">
          <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted">{c.signalsLabel}</p>
          <div className="mt-7 grid gap-5 sm:grid-cols-2 lg:grid-cols-4">
            {c.signals.map(([value, label, body]) => (
              <div key={label} className="rounded-[22px] border border-border bg-surface p-6">
                <p className="font-display text-[clamp(28px,3vw,38px)] font-semibold tracking-[-0.02em]">{value}</p>
                <p className="mt-1 text-sm font-semibold">{label}</p>
                <p className="mt-3 text-[13px] leading-relaxed text-muted">{body}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Where ShogunAI sits */}
      <section className="py-[clamp(44px,7vw,88px)]">
        <div className="container-x grid gap-10 lg:grid-cols-2">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{c.thesisLabel}</p>
            <h2 className="mt-3 font-display text-[clamp(26px,4vw,40px)] font-semibold tracking-[-0.02em]">{c.thesisTitle}</h2>
            <p className="mt-4 text-[16px] leading-relaxed text-muted">{c.thesisBody}</p>
            <a
              className="mt-6 inline-flex items-center gap-1.5 text-sm font-semibold text-accent"
              href={`${prefix}/blog/investors-on-the-memory-and-context-layer`}
            >
              {c.readMore}
              <ArrowUpRight className="size-4" />
            </a>
          </div>
          <div className="grid gap-4 self-start">
            {c.theses.map(([title, body]) => (
              <div key={title} className="rounded-[22px] border border-border bg-surface p-6">
                <h3 className="font-display text-lg font-semibold">{title}</h3>
                <p className="mt-2.5 text-[15px] leading-relaxed text-muted">{body}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Sourcing note — the quotes are third-party opinion, never an endorsement */}
      <section className="pb-[clamp(44px,7vw,88px)]">
        <div className="container-x">
          <div className="rounded-[22px] border border-border bg-cloud/45 p-7">
            <h2 className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">{c.notesLabel}</h2>
            <p className="mt-3 max-w-[76ch] text-[14px] leading-relaxed text-muted">{c.notesBody}</p>
            <ul className="mt-5 grid gap-2 sm:grid-cols-2">
              {SOURCES.map(([label, href]) => (
                <li key={href}>
                  <a
                    href={href}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[13px] text-muted underline decoration-border underline-offset-4 transition-colors hover:text-ink"
                  >
                    {label}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>

      <CTA t={t} />
    </PageShell>
  );
}
