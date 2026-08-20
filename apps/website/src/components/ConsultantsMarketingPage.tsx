import Link from 'next/link';
import {
  ArrowRight,
  Check,
  FileText,
  KeyRound,
  LockKeyhole,
  Mail,
  PauseCircle,
  Search,
  Send,
  ShieldCheck,
  Sparkles,
  Video,
  X,
} from 'lucide-react';
import { AppFrame } from '@/components/AppFrame';
import type { Locale } from '@/i18n/config';
import type { MarketingDetail } from '@/lib/marketing-content';

type PrivacyCard = { title: string; body: string };
type VisualCopy = {
  email: string;
  meeting: string;
  proposal: string;
  memory: string;
  ask: string;
  decisions: string;
  commitments: string;
  brief: string;
  followUp: string;
  review: string;
  approved: string;
};
type ConsultantsCopy = {
  heroEyebrow: string;
  heroTitle: string;
  heroAccent: string;
  heroCta: string;
  demoTitle: string;
  proof: readonly [string, string, string];
  comparisonEyebrow: string;
  comparisonTitle: string;
  comparisonBody: string;
  oldTitle: string;
  newTitle: string;
  oldItems: readonly [string, string, string, string];
  newItems: readonly [string, string, string, string];
  privacyEyebrow: string;
  privacyTitle: string;
  privacyIntro: string;
  privacyLink: string;
  privacyCards: readonly [PrivacyCard, PrivacyCard, PrivacyCard, PrivacyCard, PrivacyCard];
  featuresEyebrow: string;
  featuresTitle: string;
  featuresBody: string;
  visual: VisualCopy;
  faqEyebrow: string;
  faqTitle: string;
  finalTitle: string;
  finalBody: string;
  finalCta: string;
};

const copyByLocale: Record<Locale, ConsultantsCopy> = {
  en: {
    heroEyebrow: 'AI-powered client work',
    heroTitle: 'Every client context, in one',
    heroAccent: 'private memory',
    heroCta: 'Get early access',
    demoTitle: 'ShogunAI private work memory',
    proof: ['Local-first memory', 'Bring your own AI', 'Approval before sending'],
    comparisonEyebrow: 'The ShogunAI way',
    comparisonTitle: 'Client work is fragmented. Let’s reconnect it.',
    comparisonBody:
      'Move from scattered context and manual reconstruction to a private memory that is ready before the work begins.',
    oldTitle: 'The old way',
    newTitle: 'The ShogunAI way',
    oldItems: [
      'Search across email, documents, meetings, and notes',
      'Rebuild the client story before every conversation',
      'Carry commitments and follow-ups in your head',
      'Pay the attention cost every time you switch clients',
    ],
    newItems: [
      'Recall client context from your private work memory',
      'Prepare a briefing from context you already have',
      'Draft follow-ups and review them before they leave',
      'Keep memory local by default and control what is shared',
    ],
    privacyEyebrow: 'Privacy & control',
    privacyTitle: 'Client context stays under your control',
    privacyIntro:
      'ShogunAI is designed around local-first memory and explicit choices about what is captured, shared, or acted on.',
    privacyLink: 'See our privacy and security approach',
    privacyCards: [
      {
        title: 'Private client memory, local first',
        body: 'Captured work memory stays on your Mac by default, so your client history does not need a permanent cloud copy.',
      },
      {
        title: 'Pause or remove memory',
        body: 'Stop capture when needed and remove local memory on your terms.',
      },
      { title: 'Never used for training', body: 'Your work context is not used to train our models.' },
      {
        title: 'Bring your own provider',
        body: 'Choose a supported AI provider and manage the relationship with your own key.',
      },
      {
        title: 'Approval before consequence',
        body: 'Client-facing sends and other consequential actions pause for your review.',
      },
    ],
    featuresEyebrow: 'AI for the complete client workflow',
    featuresTitle: 'From scattered signals to client-ready work',
    featuresBody:
      'ShogunAI connects the context behind each client, prepares the next conversation, and helps finish the follow-through.',
    visual: {
      email: 'Email',
      meeting: 'Meeting',
      proposal: 'Proposal',
      memory: 'Client memory',
      ask: 'What changed since our last call?',
      decisions: 'Recent decisions',
      commitments: 'Open commitments',
      brief: 'Brief ready',
      followUp: 'Client follow-up',
      review: 'Review required',
      approved: 'Approved',
    },
    faqEyebrow: 'Questions, answered',
    faqTitle: 'Clear answers before client context enters the workflow',
    finalTitle: 'Manage more clients without losing the thread',
    finalBody: 'Keep every conversation, commitment, and next step connected—then turn that context into work.',
    finalCta: 'Get early access',
  },
  ja: {
    heroEyebrow: 'AIで進める顧客業務',
    heroTitle: 'すべての顧客文脈を、ひとつの',
    heroAccent: 'プライベートな記憶へ',
    heroCta: '早期アクセスを申し込む',
    demoTitle: 'ShogunAIのプライベートな仕事の記憶',
    proof: ['ローカルファースト', '利用するAIを選択', '送信前に承認'],
    comparisonEyebrow: 'ShogunAIの進め方',
    comparisonTitle: '分散した顧客業務を、ひとつの流れへ。',
    comparisonBody: '散らばった文脈を毎回組み立て直す働き方から、仕事を始める前に必要な経緯がそろう働き方へ変えます。',
    oldTitle: 'これまでの顧客業務',
    newTitle: 'ShogunAIなら',
    oldItems: [
      'メール、文書、会議、メモを横断して探す',
      '会話のたびに顧客の経緯を組み立て直す',
      '約束事項とフォローを頭の中だけで抱える',
      '顧客を切り替えるたびに集中力を使う',
    ],
    newItems: [
      'プライベートな仕事の記憶から顧客文脈を呼び出す',
      'すでに持っている文脈から会議ブリーフを準備する',
      'フォローを下書きし、外部へ出る前に確認する',
      '記憶を既定でローカルに保ち、共有範囲を管理する',
    ],
    privacyEyebrow: 'プライバシーと管理',
    privacyTitle: '顧客の文脈を、あなたの管理下に',
    privacyIntro:
      'ShogunAIはローカルファーストな記憶を中心に、何を取得し、共有し、実行するかを明示的に選べるよう設計されています。',
    privacyLink: 'プライバシーと安全性への取り組み',
    privacyCards: [
      {
        title: '顧客の記憶はローカルファースト',
        body: '取得した仕事の記憶は既定でMac内に保たれ、顧客履歴を恒久的にクラウドへ複製する必要を減らします。',
      },
      {
        title: 'いつでも停止・削除',
        body: '必要なときに取得を止め、ローカルの記憶を自分の判断で削除できます。',
      },
      { title: '学習には利用しない', body: '仕事の文脈をShogunAIのモデル学習には使用しません。' },
      {
        title: '利用するAIを自分で選ぶ',
        body: '対応するAIプロバイダを選び、自分のAPIキーで関係を管理できます。',
      },
      {
        title: '重要操作は承認してから',
        body: '顧客への送信など影響のある操作は、確認してから実行されます。',
      },
    ],
    featuresEyebrow: '顧客業務全体を支えるAI',
    featuresTitle: '散らばった兆しを、顧客に届ける仕事へ',
    featuresBody: '顧客ごとの経緯をつなぎ、次の会話を準備し、その後のフォローまで同じ文脈で進めます。',
    visual: {
      email: 'メール',
      meeting: '会議',
      proposal: '提案書',
      memory: '顧客の記憶',
      ask: '前回の会話から何が変わった？',
      decisions: '最近の判断',
      commitments: '未完了の約束',
      brief: 'ブリーフ完成',
      followUp: '顧客フォロー',
      review: '確認が必要',
      approved: '承認済み',
    },
    faqEyebrow: 'よくある質問',
    faqTitle: '顧客の文脈を扱う前に知っておきたいこと',
    finalTitle: '顧客を増やしても、経緯を見失わない',
    finalBody: '会話、約束、次のアクションをひとつの文脈につなぎ、そのまま仕事へ変えます。',
    finalCta: '早期アクセスを申し込む',
  },
  es: {
    heroEyebrow: 'Trabajo con clientes impulsado por IA',
    heroTitle: 'Todo el contexto de tus clientes, en una',
    heroAccent: 'memoria privada',
    heroCta: 'Solicitar acceso anticipado',
    demoTitle: 'Memoria privada de trabajo de ShogunAI',
    proof: ['Memoria local-first', 'Elige tu IA', 'Aprobación antes de enviar'],
    comparisonEyebrow: 'La forma ShogunAI',
    comparisonTitle: 'El trabajo con clientes está fragmentado. Volvamos a conectarlo.',
    comparisonBody:
      'Pasa de reconstruir contexto disperso a tener una memoria privada lista antes de empezar el trabajo.',
    oldTitle: 'La forma anterior',
    newTitle: 'La forma ShogunAI',
    oldItems: [
      'Buscar entre correo, documentos, reuniones y notas',
      'Reconstruir la historia del cliente antes de cada conversación',
      'Guardar compromisos y seguimientos solo en tu cabeza',
      'Pagar el coste de atención cada vez que cambias de cliente',
    ],
    newItems: [
      'Recuperar contexto desde tu memoria privada de trabajo',
      'Preparar un briefing con el contexto que ya tienes',
      'Redactar seguimientos y revisarlos antes de enviarlos',
      'Mantener la memoria en local y controlar lo que se comparte',
    ],
    privacyEyebrow: 'Privacidad y control',
    privacyTitle: 'El contexto de tus clientes sigue bajo tu control',
    privacyIntro:
      'ShogunAI parte de una memoria local-first y de decisiones explícitas sobre qué se captura, comparte o ejecuta.',
    privacyLink: 'Ver nuestro enfoque de privacidad y seguridad',
    privacyCards: [
      {
        title: 'Memoria privada, primero en local',
        body: 'La memoria de trabajo permanece en tu Mac por defecto, sin exigir una copia permanente del historial de tus clientes en la nube.',
      },
      {
        title: 'Pausa o elimina la memoria',
        body: 'Detén la captura cuando lo necesites y elimina la memoria local bajo tus condiciones.',
      },
      {
        title: 'Nunca se usa para entrenar',
        body: 'El contexto de tu trabajo no se utiliza para entrenar nuestros modelos.',
      },
      {
        title: 'Elige tu proveedor',
        body: 'Escoge un proveedor de IA compatible y gestiona la conexión con tu propia clave.',
      },
      {
        title: 'Aprobación antes de actuar',
        body: 'Los envíos a clientes y otras acciones relevantes se detienen para que los revises.',
      },
    ],
    featuresEyebrow: 'IA para todo el flujo de clientes',
    featuresTitle: 'De señales dispersas a trabajo listo para el cliente',
    featuresBody:
      'ShogunAI conecta el contexto de cada cliente, prepara la siguiente conversación y ayuda a completar el seguimiento.',
    visual: {
      email: 'Correo',
      meeting: 'Reunión',
      proposal: 'Propuesta',
      memory: 'Memoria del cliente',
      ask: '¿Qué cambió desde la última llamada?',
      decisions: 'Decisiones recientes',
      commitments: 'Compromisos abiertos',
      brief: 'Briefing listo',
      followUp: 'Seguimiento del cliente',
      review: 'Revisión necesaria',
      approved: 'Aprobado',
    },
    faqEyebrow: 'Preguntas frecuentes',
    faqTitle: 'Respuestas claras antes de incorporar contexto de clientes',
    finalTitle: 'Gestiona más clientes sin perder el hilo',
    finalBody: 'Conecta conversaciones, compromisos y próximos pasos, y convierte ese contexto en trabajo terminado.',
    finalCta: 'Solicitar acceso anticipado',
  },
  de: {
    heroEyebrow: 'KI-gestützte Kundenarbeit',
    heroTitle: 'Jeder Kundenkontext in einem',
    heroAccent: 'privaten Gedächtnis',
    heroCta: 'Frühzugang anfragen',
    demoTitle: 'Privates Arbeitsgedächtnis von ShogunAI',
    proof: ['Local-first-Gedächtnis', 'Eigene KI wählen', 'Freigabe vor dem Senden'],
    comparisonEyebrow: 'Die ShogunAI-Arbeitsweise',
    comparisonTitle: 'Kundenarbeit ist fragmentiert. Verbinden wir sie wieder.',
    comparisonBody:
      'Wechsle vom manuellen Zusammensetzen verstreuten Kontexts zu einem privaten Gedächtnis, das vor Arbeitsbeginn bereitsteht.',
    oldTitle: 'Die bisherige Arbeitsweise',
    newTitle: 'Die ShogunAI-Arbeitsweise',
    oldItems: [
      'In E-Mails, Dokumenten, Meetings und Notizen suchen',
      'Die Kundengeschichte vor jedem Gespräch neu zusammensetzen',
      'Zusagen und Follow-ups nur im Kopf behalten',
      'Bei jedem Kundenwechsel erneut Aufmerksamkeit aufbringen',
    ],
    newItems: [
      'Kundenkontext aus dem privaten Arbeitsgedächtnis abrufen',
      'Briefings aus bereits vorhandenem Kontext vorbereiten',
      'Follow-ups entwerfen und vor dem Versand prüfen',
      'Gedächtnis standardmäßig lokal halten und Freigaben steuern',
    ],
    privacyEyebrow: 'Datenschutz & Kontrolle',
    privacyTitle: 'Kundenkontext bleibt unter deiner Kontrolle',
    privacyIntro:
      'ShogunAI setzt auf ein Local-first-Gedächtnis und klare Entscheidungen darüber, was erfasst, geteilt oder ausgeführt wird.',
    privacyLink: 'Unseren Datenschutz- und Sicherheitsansatz ansehen',
    privacyCards: [
      {
        title: 'Privates Gedächtnis, zuerst lokal',
        body: 'Erfasster Arbeitskontext bleibt standardmäßig auf deinem Mac, ohne dauerhafte Cloud-Kopie der Kundenhistorie.',
      },
      {
        title: 'Gedächtnis pausieren oder löschen',
        body: 'Stoppe die Erfassung bei Bedarf und entferne lokale Erinnerungen zu deinen Bedingungen.',
      },
      {
        title: 'Nie für Training verwendet',
        body: 'Dein Arbeitskontext wird nicht zum Training unserer Modelle verwendet.',
      },
      {
        title: 'Eigenen Anbieter wählen',
        body: 'Wähle einen unterstützten KI-Anbieter und verwalte die Verbindung mit deinem eigenen Schlüssel.',
      },
      {
        title: 'Freigabe vor der Ausführung',
        body: 'Kundensendungen und andere folgenreiche Aktionen warten auf deine Prüfung.',
      },
    ],
    featuresEyebrow: 'KI für den gesamten Kundenworkflow',
    featuresTitle: 'Von verstreuten Signalen zu kundenfertiger Arbeit',
    featuresBody:
      'ShogunAI verbindet den Kontext jedes Kunden, bereitet das nächste Gespräch vor und unterstützt das Follow-up.',
    visual: {
      email: 'E-Mail',
      meeting: 'Meeting',
      proposal: 'Angebot',
      memory: 'Kundengedächtnis',
      ask: 'Was hat sich seit dem letzten Gespräch geändert?',
      decisions: 'Aktuelle Entscheidungen',
      commitments: 'Offene Zusagen',
      brief: 'Briefing bereit',
      followUp: 'Kunden-Follow-up',
      review: 'Prüfung erforderlich',
      approved: 'Freigegeben',
    },
    faqEyebrow: 'Häufige Fragen',
    faqTitle: 'Klare Antworten, bevor Kundenkontext in den Workflow gelangt',
    finalTitle: 'Mehr Kunden betreuen, ohne den Faden zu verlieren',
    finalBody:
      'Verbinde Gespräche, Zusagen und nächste Schritte und verwandle diesen Kontext direkt in erledigte Arbeit.',
    finalCta: 'Frühzugang anfragen',
  },
};

const privacyIcons = [LockKeyhole, PauseCircle, ShieldCheck, KeyRound, Check];
const privacyAccents = ['#6758ff', '#087c62', '#68aef5', '#48c979'];

function MemoryVisual({ copy }: { copy: VisualCopy }) {
  const sources = [
    { label: copy.email, Icon: Mail, color: 'text-[#72a6ff]' },
    { label: copy.meeting, Icon: Video, color: 'text-[#bf7bff]' },
    { label: copy.proposal, Icon: FileText, color: 'text-[#ff85c2]' },
  ];
  return (
    <div className="relative flex h-[250px] items-center justify-center overflow-hidden rounded-[20px] bg-[#090a0f] p-5 text-white">
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_50%_55%,rgba(115,73,255,0.38),transparent_38%)]" />
      <div className="relative flex w-full items-center justify-between gap-2">
        {sources.map(({ label, Icon, color }) => (
          <div key={label} className="flex min-w-0 flex-1 flex-col items-center gap-2">
            <span className="flex size-11 items-center justify-center rounded-xl border border-white/10 bg-white/5">
              <Icon className={`size-5 ${color}`} strokeWidth={1.8} aria-hidden="true" />
            </span>
            <span className="max-w-full truncate text-[11px] text-white/55">{label}</span>
          </div>
        ))}
      </div>
      <div className="absolute left-1/2 top-1/2 flex size-[104px] -translate-x-1/2 -translate-y-1/2 flex-col items-center justify-center rounded-full border border-[#b991ff]/45 bg-[radial-gradient(circle,#733cff,#321d69)] shadow-[0_0_54px_rgba(130,73,255,0.48)]">
        <Sparkles className="size-6" aria-hidden="true" />
        <span className="mt-1 max-w-[80px] text-center text-[11px] font-semibold leading-tight">{copy.memory}</span>
      </div>
    </div>
  );
}

function BriefVisual({ copy }: { copy: VisualCopy }) {
  return (
    <div className="h-[250px] overflow-hidden rounded-[20px] bg-[#090a0f] p-5 text-white">
      <div className="flex items-center gap-2 rounded-xl border border-white/10 bg-white/[0.06] px-3 py-3">
        <Search className="size-4 shrink-0 text-[#b985ff]" aria-hidden="true" />
        <span className="truncate text-[11px] text-white/60">{copy.ask}</span>
        <Sparkles className="ml-auto size-4 shrink-0 text-[#ff75d1]" aria-hidden="true" />
      </div>
      <div className="mt-4 grid gap-2">
        {[copy.decisions, copy.commitments].map((label, index) => (
          <div key={label} className="border-white/8 flex items-center gap-3 rounded-xl border bg-white/[0.035] p-3">
            <span className={`size-2 rounded-full ${index === 0 ? 'bg-[#7965ff]' : 'bg-[#45c978]'}`} />
            <span className="text-[11px] text-white/70">{label}</span>
            <span className="ml-auto h-1.5 w-12 rounded-full bg-white/10" />
          </div>
        ))}
      </div>
      <div className="mt-4 flex items-center gap-2 rounded-xl bg-[linear-gradient(110deg,#5b35d9,#b03da2)] px-4 py-3 text-[12px] font-semibold shadow-[0_14px_35px_rgba(106,56,218,0.3)]">
        <Check className="size-4" aria-hidden="true" />
        {copy.brief}
      </div>
    </div>
  );
}

function FollowUpVisual({ copy }: { copy: VisualCopy }) {
  return (
    <div className="h-[250px] overflow-hidden rounded-[20px] bg-[#090a0f] p-5 text-white">
      <div className="rounded-2xl border border-white/10 bg-white/[0.045] p-4">
        <div className="flex items-center gap-3">
          <span className="flex size-9 items-center justify-center rounded-xl bg-[#6438d8]">
            <Send className="size-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="truncate text-[12px] font-semibold">{copy.followUp}</p>
            <p className="mt-1 text-[10px] text-white/45">ShogunAI</p>
          </div>
        </div>
        <div className="mt-4 space-y-2">
          <span className="block h-2 w-full rounded-full bg-white/10" />
          <span className="block h-2 w-[86%] rounded-full bg-white/10" />
          <span className="block h-2 w-[62%] rounded-full bg-white/10" />
        </div>
        <div className="mt-5 flex items-center justify-between gap-2">
          <span className="rounded-full border border-[#ffbd64]/25 bg-[#ffbd64]/10 px-3 py-1.5 text-[10px] font-medium text-[#ffd18e]">
            {copy.review}
          </span>
          <span className="flex items-center gap-1.5 rounded-lg bg-[#42bd70] px-3 py-1.5 text-[10px] font-semibold text-[#07130b]">
            <Check className="size-3" aria-hidden="true" />
            {copy.approved}
          </span>
        </div>
      </div>
    </div>
  );
}

export function ConsultantsMarketingPage({ page, locale }: { page: MarketingDetail; locale: Locale }) {
  const copy = copyByLocale[locale];
  const homeCta = `/${locale}/#get-started`;
  const featureVisuals = [MemoryVisual, BriefVisual, FollowUpVisual];

  return (
    <div className="bg-bg overflow-hidden">
      <header className="border-border bg-bg relative border-b py-[clamp(68px,8vw,118px)]">
        <div
          className="absolute inset-x-0 top-0 h-[420px] bg-[radial-gradient(ellipse_at_70%_0%,rgba(110,82,255,0.12),transparent_55%)]"
          aria-hidden="true"
        />
        <div className="container-x relative grid gap-14 lg:grid-cols-[minmax(0,0.88fr)_minmax(0,1.12fr)] lg:items-center lg:gap-[clamp(56px,6vw,96px)]">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.15em] text-[#6758ff]">{copy.heroEyebrow}</p>
            <h1 className="text-ink mt-6 max-w-[11ch] text-balance font-sans text-[clamp(48px,6vw,80px)] font-semibold leading-[0.96] tracking-[-0.065em]">
              {copy.heroTitle}{' '}
              <span className="bg-[linear-gradient(95deg,#25252b_5%,#8b8b94_65%,#b0b0b8)] bg-clip-text text-transparent [html[data-theme='dark']_&]:bg-[linear-gradient(95deg,#ffffff_5%,#a8a8b5_75%)] [html[data-theme='dark']_&]:bg-clip-text">
                {copy.heroAccent}
              </span>
            </h1>
            <p className="text-muted mt-7 max-w-[640px] text-[clamp(17px,1.55vw,21px)] leading-[1.6]">
              {page.description}
            </p>
            <Link
              href={homeCta}
              className="bg-ink text-bg mt-8 inline-flex min-h-14 items-center justify-center gap-3 rounded-[14px] px-7 text-[15px] font-semibold shadow-[0_14px_32px_rgba(18,18,23,0.16)] transition-transform hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#6758ff] focus-visible:ring-offset-4"
            >
              {copy.heroCta}
              <ArrowRight className="size-4" />
            </Link>
            <ul className="mt-7 flex flex-wrap gap-x-5 gap-y-2">
              {copy.proof.map((item) => (
                <li key={item} className="text-muted flex items-center gap-2 text-[12px] font-medium">
                  <Check className="size-3.5 text-[#35ad67]" strokeWidth={3} aria-hidden="true" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
          <div className="mx-auto w-full max-w-[760px] lg:max-w-none">
            <AppFrame src="/app-demo/panel.html" title={copy.demoTitle} device="mac" className="w-full" />
          </div>
        </div>
      </header>

      <section className="py-[clamp(72px,9vw,126px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[940px] text-center">
            <p className="text-xs font-semibold uppercase tracking-[0.15em] text-[#6758ff]">{copy.comparisonEyebrow}</p>
            <h2 className="text-ink mt-5 text-balance font-sans text-[clamp(38px,5.3vw,68px)] font-semibold leading-[1] tracking-[-0.06em]">
              {copy.comparisonTitle}
            </h2>
            <p className="text-muted mx-auto mt-6 max-w-[760px] text-[clamp(16px,1.35vw,19px)] leading-[1.65]">
              {copy.comparisonBody}
            </p>
          </div>
          <div className="theme-light-panel border-border bg-surface mt-14 grid overflow-hidden rounded-[26px] border lg:grid-cols-2">
            <article className="lg:border-border p-[clamp(28px,4.5vw,58px)] lg:border-r">
              <h3 className="text-muted text-[clamp(25px,2.7vw,36px)] font-semibold tracking-[-0.035em]">
                {copy.oldTitle}
              </h3>
              <ul className="mt-8 grid gap-5">
                {copy.oldItems.map((item) => (
                  <li
                    key={item}
                    className="text-muted flex items-start gap-4 text-[clamp(15px,1.25vw,18px)] leading-[1.55]"
                  >
                    <X className="mt-1 size-5 shrink-0 text-[#ef4d48]" strokeWidth={2.5} aria-hidden="true" />
                    {item}
                  </li>
                ))}
              </ul>
            </article>
            <article className="theme-soft-section bg-[#f7f4ff] p-[clamp(28px,4.5vw,58px)]">
              <h3 className="text-ink text-[clamp(25px,2.7vw,36px)] font-semibold tracking-[-0.035em]">
                {copy.newTitle}
              </h3>
              <ul className="mt-8 grid gap-5">
                {copy.newItems.map((item) => (
                  <li
                    key={item}
                    className="text-ink flex items-start gap-4 text-[clamp(15px,1.25vw,18px)] font-medium leading-[1.55]"
                  >
                    <Check className="mt-1 size-5 shrink-0 text-[#25a65a]" strokeWidth={2.8} aria-hidden="true" />
                    {item}
                  </li>
                ))}
              </ul>
              <Link
                href={homeCta}
                className="bg-ink text-bg mt-9 inline-flex items-center gap-2 rounded-xl px-5 py-3 text-sm font-semibold"
              >
                {copy.heroCta}
                <ArrowRight className="size-4" />
              </Link>
            </article>
          </div>
        </div>
      </section>

      <section className="border-border bg-cloud/35 border-y py-[clamp(72px,9vw,122px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[920px] text-center">
            <p className="text-xs font-semibold uppercase tracking-[0.15em] text-[#6758ff]">{copy.featuresEyebrow}</p>
            <h2 className="text-ink mt-5 text-balance font-sans text-[clamp(38px,5.2vw,66px)] font-semibold leading-[1] tracking-[-0.06em]">
              {copy.featuresTitle}
            </h2>
            <p className="text-muted mx-auto mt-6 max-w-[760px] text-[clamp(16px,1.35vw,19px)] leading-[1.65]">
              {copy.featuresBody}
            </p>
          </div>
          <div className="mt-14 grid gap-5 md:grid-cols-3">
            {page.steps.map((step, index) => {
              const Visual = featureVisuals[index];
              return (
                <article
                  key={step.title}
                  className="theme-light-panel border-border bg-surface rounded-[24px] border p-4 shadow-[0_18px_50px_rgba(19,22,30,0.06)] sm:p-5"
                >
                  <Visual copy={copy.visual} />
                  <div className="px-2 pb-3 pt-7">
                    <h3 className="text-ink text-[clamp(22px,2vw,29px)] font-semibold leading-[1.08] tracking-[-0.04em]">
                      {step.title}
                    </h3>
                    <p className="text-muted mt-4 text-[15px] leading-[1.65]">{step.body}</p>
                  </div>
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section className="py-[clamp(72px,10vw,136px)]">
        <div className="container-x">
          <div className="grid gap-10 lg:grid-cols-[minmax(0,0.92fr)_minmax(0,1.08fr)] lg:items-start lg:gap-16">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.15em] text-[#6758ff]">{copy.privacyEyebrow}</p>
              <h2 className="text-ink mt-5 max-w-[12ch] text-balance font-sans text-[clamp(42px,5.4vw,72px)] font-semibold leading-[0.98] tracking-[-0.06em]">
                {copy.privacyTitle}
              </h2>
              <p className="text-muted mt-7 max-w-[620px] text-[clamp(16px,1.4vw,19px)] leading-[1.65]">
                {copy.privacyIntro}
              </p>
              <article className="theme-soft-section mt-10 rounded-[28px] bg-[#f7f4ff] p-[clamp(28px,4vw,54px)]">
                <LockKeyhole className="text-ink size-14" strokeWidth={1.55} aria-hidden="true" />
                <h3 className="text-ink mt-8 max-w-[15ch] text-balance text-[clamp(30px,3.4vw,48px)] font-semibold leading-[1.02] tracking-[-0.05em]">
                  {copy.privacyCards[0].title}
                </h3>
                <p className="text-muted mt-5 max-w-[580px] text-[16px] leading-[1.65]">{copy.privacyCards[0].body}</p>
                <Link
                  href={`/${locale}/security`}
                  className="mt-8 inline-flex items-center gap-2 text-sm font-semibold text-[#6758ff] underline decoration-[#6758ff]/30 underline-offset-4 hover:decoration-[#6758ff]"
                >
                  {copy.privacyLink}
                  <ArrowRight className="size-4" />
                </Link>
              </article>
            </div>
            <div className="grid gap-5 sm:grid-cols-2 lg:pt-10">
              {copy.privacyCards.slice(1).map((card, index) => {
                const Icon = privacyIcons[index + 1];
                return (
                  <article
                    key={card.title}
                    className="theme-light-panel border-border bg-surface relative min-h-[300px] overflow-hidden rounded-[24px] border p-8 sm:min-h-[340px]"
                  >
                    <span className="absolute inset-x-0 top-0 h-2" style={{ backgroundColor: privacyAccents[index] }} />
                    <Icon className="text-ink size-10" strokeWidth={1.7} aria-hidden="true" />
                    <h3 className="text-ink mt-9 text-balance text-[clamp(25px,2.3vw,34px)] font-semibold leading-[1.04] tracking-[-0.045em]">
                      {card.title}
                    </h3>
                    <p className="text-muted mt-5 text-[15px] leading-[1.65]">{card.body}</p>
                  </article>
                );
              })}
            </div>
          </div>
        </div>
      </section>

      <section className="border-border bg-cloud/45 border-y py-[clamp(64px,8vw,104px)]">
        <div className="container-x max-w-[860px]">
          <p className="text-center text-xs font-semibold uppercase tracking-[0.15em] text-[#6758ff]">
            {copy.faqEyebrow}
          </p>
          <h2 className="text-ink mx-auto mt-5 max-w-[17ch] text-balance text-center font-sans text-[clamp(36px,4.5vw,56px)] font-semibold leading-[1.02] tracking-[-0.055em]">
            {copy.faqTitle}
          </h2>
          <div className="mt-10 grid gap-3">
            {page.faq.map(([question, answer]) => (
              <details
                key={question}
                className="border-border bg-surface group rounded-2xl border px-6 open:shadow-[var(--shadow-card)]"
              >
                <summary className="cursor-pointer list-none py-5 font-semibold [&::-webkit-details-marker]:hidden">
                  {question}
                </summary>
                <p className="text-muted pb-6 text-[15px] leading-relaxed">{answer}</p>
              </details>
            ))}
          </div>
        </div>
      </section>

      <section className="py-[clamp(64px,8vw,112px)]">
        <div className="container-x">
          <div className="relative flex min-h-[480px] items-center justify-center overflow-hidden rounded-[30px] bg-[url('/optimized/shogunai-hero-kyoto-v3.jpg')] bg-cover bg-center px-6 py-16 text-center text-white sm:min-h-[540px]">
            <div className="absolute inset-0 bg-[linear-gradient(115deg,rgba(3,18,37,0.8),rgba(62,34,118,0.48)_55%,rgba(4,16,31,0.74))]" />
            <div className="relative mx-auto max-w-[900px]">
              <h2 className="text-balance font-sans text-[clamp(42px,6vw,74px)] font-semibold leading-[0.97] tracking-[-0.065em]">
                {copy.finalTitle}
              </h2>
              <p className="mx-auto mt-7 max-w-[680px] text-[clamp(17px,1.7vw,21px)] leading-[1.6] text-white/85">
                {copy.finalBody}
              </p>
              <Link
                href={homeCta}
                className="mt-9 inline-flex min-h-14 items-center justify-center gap-3 rounded-[14px] bg-white px-8 text-[15px] font-semibold text-[#07131f] transition-transform hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white focus-visible:ring-offset-4 focus-visible:ring-offset-[#08354f]"
              >
                {copy.finalCta}
                <ArrowRight className="size-4" />
              </Link>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
