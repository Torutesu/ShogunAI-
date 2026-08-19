import type { Metadata } from 'next';
import { ArrowUpRight } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { PostCard } from '@/components/PostCard';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { getPost } from '@/lib/blog';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Market — What investors are saying about the memory & context layer',
  description:
    'Sourced public commentary from a16z, Emergence Capital, Kindred Ventures, Wing and others on memory, context and agents as coworkers — and where ShogunAI fits.',
  alternates: { canonical: '/en/market', languages: localizedAlternates('/market') },
};

/**
 * Our own pieces on this category, newest thinking first. Everything the cards
 * show (title, description, cover, reading time) comes from the posts
 * themselves via `getPost`, so editing an article updates this page too.
 */
const FEATURED_SLUGS = [
  'altman-on-forgetting',
  'garry-tan-own-your-intelligence',
  'seci-for-the-ai-native-individual',
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
      'So we wrote down our own reading of it. Three pieces: a diagnosis from the CEO holding the most powerful model, a prescription from the president of Y Combinator, and a thirty-year-old theory of knowledge creation that only now has the tooling it always assumed. Each arrives at the same place from a different direction.',
    ],
    featuredLabel: 'Our reading of the category',
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
    notesBody: 'Every quote on this page is taken from a public post or press release by the firm named, and the full list of sources is below. These are investor theses about where the market is going — opinions about the future, not settled facts — and none of them are statements about ShogunAI.',
  },
  ja: {
    eyebrow: '市場',
    title: 'メモリと文脈は、ひとつの市場になりつつある。',
    lead: [
      'この2年ほど、ボトルネックはモデルでした。もうそうではありません。エージェント基盤に出資するファンドの視線は、別の制約に集まっています ── エージェントの有用性は、届く文脈の広さで決まる。そして、ほとんどの人の文脈はどこにも届いていない。',
      'そこで、私たち自身の読み方を記事にしました。最も強力なモデルを持つ会社のCEOが下した診断、Y Combinator の社長が出した処方、そして前提としていた道具がようやく揃った30年前の知識創造理論 ── この3本です。どれも違う方向から、同じ場所に着きます。',
    ],
    featuredLabel: '私たちのこのカテゴリの読み方',
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
    notesBody: 'このページの引用はすべて、記載した各社の公開記事やプレスリリースからのもので、出典は下にすべて挙げています。いずれも市場の行き先についての投資家の仮説 ── 将来についての意見であって、確定した事実ではありません ── であり、ShogunAI について述べたものではありません。',
  },
  es: {
    eyebrow: 'Mercado',
    title: 'La memoria y el contexto se están convirtiendo en un mercado propio.',
    lead: [
      'Durante casi dos años el cuello de botella fue el modelo. Ya no lo es. Los fondos que financian la infraestructura de agentes han convergido en otra restricción: un agente sirve tanto como el contexto que alcanza, y casi nadie tiene su contexto al alcance.',
      'Así que escribimos nuestra propia lectura. Tres piezas: el diagnóstico del CEO que tiene el modelo más potente, la receta del presidente de Y Combinator y una teoría de creación de conocimiento de hace treinta años que por fin dispone de las herramientas que siempre dio por supuestas. Las tres llegan al mismo sitio por caminos distintos.',
    ],
    featuredLabel: 'Nuestra lectura de la categoría',
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
    notesBody: 'Cada cita procede de una publicación o nota de prensa pública de la firma citada, y la lista completa de fuentes está abajo. Son tesis de inversores sobre hacia dónde va el mercado — opiniones sobre el futuro, no hechos establecidos — y ninguna se refiere a ShogunAI.',
  },
  de: {
    eyebrow: 'Markt',
    title: 'Memory und Kontext werden zu einem eigenen Markt.',
    lead: [
      'Fast zwei Jahre lang war das Modell der Engpass. Das ist vorbei. Die Fonds, die Agenten-Infrastruktur finanzieren, sind sich über eine andere Grenze einig: Ein Agent ist nur so nützlich wie der Kontext, den er erreicht — und kaum jemandes Kontext ist erreichbar.',
      'Also haben wir unsere eigene Lesart aufgeschrieben. Drei Texte: die Diagnose des CEO mit dem stärksten Modell, das Rezept des Y-Combinator-Präsidenten und eine dreißig Jahre alte Theorie der Wissensschaffung, die erst jetzt das Werkzeug hat, das sie immer voraussetzte. Alle drei kommen aus verschiedenen Richtungen am selben Punkt an.',
    ],
    featuredLabel: 'Unsere Lesart der Kategorie',
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
    notesBody: 'Jedes Zitat stammt aus einem öffentlichen Beitrag oder einer Pressemitteilung der genannten Firma; die vollständige Quellenliste steht unten. Es sind Investorenthesen über die Richtung des Marktes — Meinungen über die Zukunft, keine gesicherten Fakten — und keine davon handelt von ShogunAI.',
  },
} as const;

export default async function MarketPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const c = content[locale];
  const prefix = `/${locale}`;
  const featured = FEATURED_SLUGS.map((slug) => getPost(slug, locale)).filter((post) => post !== null);

  return (
    <PageShell locale={locale}>
      <JsonLd
        data={breadcrumbSchema([
          { name: 'Home', url: `${siteConfig.url}${prefix}` },
          { name: c.eyebrow, url: `${siteConfig.url}${prefix}/market` },
        ])}
      />

      <PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.lead[0]} />

      {/* Our own writing on this category — the whole card links to the post */}
      {featured.length > 0 && (
        <section className="py-[clamp(44px,7vw,88px)]">
          <div className="container-x">
            <div className="flex items-center gap-5">
              <p className="shrink-0 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted">{c.featuredLabel}</p>
              <div className="h-px flex-1 bg-border" />
            </div>
            <p className="mt-5 max-w-[76ch] text-[15px] leading-relaxed text-muted">{c.lead[1]}</p>
            <div className="mt-8 grid gap-6 md:grid-cols-2 lg:grid-cols-3">
              {featured.map((post) => (
                <PostCard
                  key={post.slug}
                  p={post}
                  categories={t.blog.categories}
                  locale={locale}
                  minRead={t.blog.minRead}
                  more={t.blog.readMore}
                  hrefPrefix={prefix}
                />
              ))}
            </div>
          </div>
        </section>
      )}

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
