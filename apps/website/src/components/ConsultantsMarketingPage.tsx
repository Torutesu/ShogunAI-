import Link from 'next/link';
import {
  ArrowRight,
  BriefcaseBusiness,
  Check,
  Database,
  Handshake,
  Headphones,
  KeyRound,
  LockKeyhole,
  PauseCircle,
  Search,
  Send,
  ShieldCheck,
} from 'lucide-react';
import type { Locale } from '@/i18n/config';
import type { MarketingDetail } from '@/lib/marketing-content';

type PrivacyCard = { title: string; body: string };
type ConsultantsCopy = {
  heroCta: string;
  benefitsEyebrow: string;
  benefitsTitle: string;
  privacyEyebrow: string;
  privacyTitle: string;
  privacyIntro: string;
  privacyLink: string;
  privacyCards: readonly [PrivacyCard, PrivacyCard, PrivacyCard, PrivacyCard, PrivacyCard];
  featuresEyebrow: string;
  featuresTitle: string;
  faqEyebrow: string;
  faqTitle: string;
  finalTitle: string;
  finalBody: string;
  finalCta: string;
};

const copyByLocale: Record<Locale, ConsultantsCopy> = {
  en: {
    heroCta: 'Get early access',
    benefitsEyebrow: 'Built for client work',
    benefitsTitle: 'Built around the way client work actually happens',
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
      {
        title: 'Never used for training',
        body: 'Your work context is not used to train our models.',
      },
      {
        title: 'Bring your own provider',
        body: 'Choose a supported AI provider and manage the relationship with your own key.',
      },
      {
        title: 'Approval before consequence',
        body: 'Client-facing sends and other consequential actions pause for your review.',
      },
    ],
    featuresEyebrow: 'From memory to delivery',
    featuresTitle: 'Move client work forward without rebuilding the backstory',
    faqEyebrow: 'Questions, answered',
    faqTitle: 'Clear answers before client context enters the workflow',
    finalTitle: 'Manage more clients without losing the thread',
    finalBody:
      'Keep every conversation, commitment, and next step connected—then turn that context into work.',
    finalCta: 'Get early access',
  },
  ja: {
    heroCta: '早期アクセスを申し込む',
    benefitsEyebrow: '顧客業務のために',
    benefitsTitle: '顧客業務の実際の流れに合わせて設計',
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
    featuresEyebrow: '記憶から成果物へ',
    featuresTitle: '経緯を組み立て直さず、顧客業務を前へ進める',
    faqEyebrow: 'よくある質問',
    faqTitle: '顧客の文脈を扱う前に知っておきたいこと',
    finalTitle: '顧客を増やしても、経緯を見失わない',
    finalBody: '会話、約束、次のアクションをひとつの文脈につなぎ、そのまま仕事へ変えます。',
    finalCta: '早期アクセスを申し込む',
  },
  es: {
    heroCta: 'Solicitar acceso anticipado',
    benefitsEyebrow: 'Hecho para el trabajo con clientes',
    benefitsTitle: 'Diseñado para cómo ocurre realmente el trabajo con clientes',
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
    featuresEyebrow: 'De la memoria a la entrega',
    featuresTitle: 'Haz avanzar el trabajo sin reconstruir toda la historia',
    faqEyebrow: 'Preguntas frecuentes',
    faqTitle: 'Respuestas claras antes de incorporar contexto de clientes',
    finalTitle: 'Gestiona más clientes sin perder el hilo',
    finalBody:
      'Conecta conversaciones, compromisos y próximos pasos, y convierte ese contexto en trabajo terminado.',
    finalCta: 'Solicitar acceso anticipado',
  },
  de: {
    heroCta: 'Frühzugang anfragen',
    benefitsEyebrow: 'Für Kundenarbeit entwickelt',
    benefitsTitle: 'Entwickelt für den tatsächlichen Ablauf von Kundenarbeit',
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
    featuresEyebrow: 'Vom Gedächtnis zur Lieferung',
    featuresTitle: 'Kundenarbeit voranbringen, ohne die Vorgeschichte neu aufzubauen',
    faqEyebrow: 'Häufige Fragen',
    faqTitle: 'Klare Antworten, bevor Kundenkontext in den Workflow gelangt',
    finalTitle: 'Mehr Kunden betreuen, ohne den Faden zu verlieren',
    finalBody:
      'Verbinde Gespräche, Zusagen und nächste Schritte und verwandle diesen Kontext direkt in erledigte Arbeit.',
    finalCta: 'Frühzugang anfragen',
  },
};

const audienceIcons = [Handshake, BriefcaseBusiness, Headphones];
const featureIcons = [Database, Search, Send];
const privacyIcons = [LockKeyhole, PauseCircle, ShieldCheck, KeyRound, Check];
const privacyAccents = ['#6558ff', '#087c62', '#68aef5', '#48c979'];
const serif = "font-[Georgia,'Times_New_Roman','Yu_Mincho','Hiragino_Mincho_ProN',serif]";

export function ConsultantsMarketingPage({
  page,
  locale,
}: {
  page: MarketingDetail;
  locale: Locale;
}) {
  const copy = copyByLocale[locale];
  const homeCta = `/${locale}/#get-started`;

  return (
    <div className="bg-bg overflow-hidden">
      <header className="theme-soft-section bg-[#fffdf6] pt-[clamp(72px,10vw,144px)] pb-[clamp(64px,8vw,112px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[940px] text-center">
            <p className="text-accent text-xs font-semibold tracking-[0.14em] uppercase">
              {page.eyebrow}
            </p>
            <h1
              className={`mx-auto mt-5 max-w-[16ch] text-balance ${serif} text-[clamp(44px,7.1vw,78px)] leading-[0.98] font-normal tracking-[-0.055em]`}
            >
              {page.title}
            </h1>
            <p className="text-muted mx-auto mt-7 max-w-[760px] text-[clamp(17px,1.8vw,21px)] leading-[1.6]">
              {page.description}
            </p>
            <Link
              href={homeCta}
              className="bg-ink text-bg focus-visible:ring-accent mt-8 inline-flex min-h-14 items-center justify-center gap-3 rounded-full px-8 text-[15px] font-semibold transition-transform hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-offset-4 focus-visible:outline-none"
            >
              {copy.heroCta}
              <ArrowRight className="size-4" />
            </Link>
          </div>

          <div className="mt-[clamp(68px,9vw,120px)] grid gap-x-10 gap-y-12 md:grid-cols-3">
            {page.highlights.map((item, index) => {
              const Icon = audienceIcons[index];
              return (
                <article key={item.title} className="flex flex-col items-center px-3 text-center">
                  <Icon className="text-ink size-9" strokeWidth={1.7} aria-hidden="true" />
                  <h2
                    className={`mt-6 text-balance ${serif} text-[clamp(27px,2.7vw,38px)] leading-[1.08] font-normal tracking-[-0.035em]`}
                  >
                    {item.title}
                  </h2>
                  <p className="text-muted mt-5 max-w-[420px] text-[clamp(15px,1.2vw,17px)] leading-[1.65]">
                    {item.body}
                  </p>
                </article>
              );
            })}
          </div>
        </div>
      </header>

      <section className="theme-soft-section mx-2 rounded-[30px] bg-[#fbf7ea] sm:mx-4 lg:mx-6">
        <div className="container-x grid gap-14 py-[clamp(68px,9vw,124px)] lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] lg:gap-[clamp(72px,9vw,144px)]">
          <div>
            <p className="text-accent text-xs font-semibold tracking-[0.14em] uppercase">
              {copy.benefitsEyebrow}
            </p>
            <h2
              className={`mt-5 max-w-[13ch] text-balance ${serif} text-[clamp(40px,5vw,68px)] leading-[1.02] font-normal tracking-[-0.05em]`}
            >
              {copy.benefitsTitle}
            </h2>
            <p className="text-muted mt-7 max-w-[590px] text-[clamp(16px,1.45vw,20px)] leading-[1.65]">
              {page.intro}
            </p>
          </div>
          <ul className="grid content-start gap-8 lg:pt-2">
            {page.outcomes.map((outcome) => (
              <li
                key={outcome}
                className="flex items-start gap-5 text-[clamp(17px,1.55vw,21px)] leading-[1.35] font-medium"
              >
                <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full bg-[#4ccd7a] text-[#07130b]">
                  <Check className="size-[18px]" strokeWidth={2.8} aria-hidden="true" />
                </span>
                <span className="max-w-[680px]">{outcome}</span>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="py-[clamp(72px,10vw,136px)]">
        <div className="container-x">
          <div className="grid gap-10 lg:grid-cols-[minmax(0,0.92fr)_minmax(0,1.08fr)] lg:items-start lg:gap-16">
            <div>
              <p className="text-accent text-xs font-semibold tracking-[0.14em] uppercase">
                {copy.privacyEyebrow}
              </p>
              <h2
                className={`mt-5 max-w-[12ch] text-balance ${serif} text-[clamp(42px,5.4vw,72px)] leading-[0.98] font-normal tracking-[-0.055em]`}
              >
                {copy.privacyTitle}
              </h2>
              <p className="text-muted mt-7 max-w-[620px] text-[clamp(16px,1.4vw,19px)] leading-[1.65]">
                {copy.privacyIntro}
              </p>
              <article className="theme-light-panel mt-10 rounded-[28px] bg-[#fbf7ea] p-[clamp(28px,4vw,54px)]">
                <LockKeyhole className="text-ink size-14" strokeWidth={1.55} aria-hidden="true" />
                <h3
                  className={`mt-8 max-w-[15ch] ${serif} text-[clamp(30px,3.4vw,48px)] leading-[1.05] font-normal tracking-[-0.04em]`}
                >
                  {copy.privacyCards[0].title}
                </h3>
                <p className="text-muted mt-5 max-w-[580px] text-[16px] leading-[1.65]">
                  {copy.privacyCards[0].body}
                </p>
                <Link
                  href={`/${locale}/security`}
                  className="text-accent decoration-accent/30 hover:decoration-accent mt-8 inline-flex items-center gap-2 text-sm font-semibold underline underline-offset-4"
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
                    className="theme-light-panel relative min-h-[300px] overflow-hidden rounded-[24px] bg-[#fbf7ea] p-8 sm:min-h-[340px]"
                  >
                    <span
                      className="absolute inset-x-0 top-0 h-2"
                      style={{ backgroundColor: privacyAccents[index] }}
                    />
                    <Icon className="text-ink size-10" strokeWidth={1.7} aria-hidden="true" />
                    <h3
                      className={`mt-9 text-balance ${serif} text-[clamp(27px,2.5vw,37px)] leading-[1.04] font-normal tracking-[-0.035em]`}
                    >
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

      <section className="theme-soft-section bg-[#fffdf6] py-[clamp(72px,9vw,120px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[850px] text-center">
            <p className="text-accent text-xs font-semibold tracking-[0.14em] uppercase">
              {copy.featuresEyebrow}
            </p>
            <h2
              className={`mt-5 text-balance ${serif} text-[clamp(38px,5vw,64px)] leading-[1.02] font-normal tracking-[-0.05em]`}
            >
              {copy.featuresTitle}
            </h2>
          </div>
          <div className="mt-16 grid gap-6 md:grid-cols-3">
            {page.steps.map((step, index) => {
              const Icon = featureIcons[index];
              return (
                <article
                  key={step.title}
                  className="theme-light-panel border-border bg-surface rounded-[24px] border p-8 sm:p-9"
                >
                  <span className="bg-sky-soft text-accent flex size-12 items-center justify-center rounded-full">
                    <Icon className="size-6" strokeWidth={1.8} aria-hidden="true" />
                  </span>
                  <h3
                    className={`mt-8 ${serif} text-[clamp(27px,2.5vw,36px)] leading-[1.07] font-normal tracking-[-0.035em]`}
                  >
                    {step.title}
                  </h3>
                  <p className="text-muted mt-5 text-[15px] leading-[1.65]">{step.body}</p>
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section className="border-border bg-cloud/45 border-y py-[clamp(64px,8vw,104px)]">
        <div className="container-x max-w-[860px]">
          <p className="text-accent text-center text-xs font-semibold tracking-[0.14em] uppercase">
            {copy.faqEyebrow}
          </p>
          <h2
            className={`mx-auto mt-5 max-w-[16ch] text-center text-balance ${serif} text-[clamp(36px,4.5vw,56px)] leading-[1.05] font-normal tracking-[-0.045em]`}
          >
            {copy.faqTitle}
          </h2>
          <div className="mt-10 grid gap-3">
            {page.faq.map(([question, answer]) => (
              <details
                key={question}
                className="group border-border bg-surface rounded-2xl border px-6 open:shadow-[var(--shadow-card)]"
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
            <div className="absolute inset-0 bg-[linear-gradient(115deg,rgba(3,18,37,0.78),rgba(0,56,89,0.46)_55%,rgba(4,16,31,0.72))]" />
            <div className="relative mx-auto max-w-[850px]">
              <h2
                className={`text-balance ${serif} text-[clamp(42px,6vw,74px)] leading-[0.98] font-normal tracking-[-0.055em]`}
              >
                {copy.finalTitle}
              </h2>
              <p className="mx-auto mt-7 max-w-[680px] text-[clamp(17px,1.7vw,21px)] leading-[1.6] text-white/85">
                {copy.finalBody}
              </p>
              <Link
                href={homeCta}
                className="mt-9 inline-flex min-h-14 items-center justify-center gap-3 rounded-full bg-white px-8 text-[15px] font-semibold text-[#07131f] transition-transform hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-white focus-visible:ring-offset-4 focus-visible:ring-offset-[#08354f] focus-visible:outline-none"
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
