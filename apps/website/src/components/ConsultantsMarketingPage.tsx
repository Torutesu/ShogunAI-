import Link from 'next/link';
import {
  ArrowRight,
  Check,
  FileText,
  Mail,
  Search,
  Send,
  Sparkles,
  Video,
  X,
} from 'lucide-react';
import { CaseCards, type CaseLabels } from '@/components/CaseCards';
import type { Locale } from '@/i18n/config';
import type { MarketingDetail } from '@/lib/marketing-content';

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
/** One before/after scene: what the day costs today, and what it costs with the memory in place. */
type Case = { title: string; before: string; lost: string; after: string };
type UseCaseCopy = {
  heroEyebrow: string;
  heroTitle: string;
  heroAccent: string;
  heroBody: string;
  heroCta: string;
  proof: readonly [string, string, string];
  comparisonEyebrow: string;
  comparisonTitle: string;
  comparisonBody: string;
  cases: readonly [Case, Case, Case, Case, Case];
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

const consultantsCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    heroEyebrow: 'A day across clients',
    heroTitle: 'Every switch costs you the time to',
    heroAccent: 'load it all back in',
    heroBody: 'Where did this project get to. What did the last one bill and how many days did it really take. What phrasing did this client dislike once before. Some days, going back to remember takes longer than the work itself.',
    heroCta: 'Get early access',
    proof: ['Local-first memory', 'Bring your own AI', 'Approval before sending'],
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Not 10% better. 10×, 100×.',
    comparisonBody:
      'Five moments from a day spent across several clients. Open a card to swap today for the same day with ShogunAI in it.',
    cases: [
      {
        title: 'Switching between clients',
        before:
          'You are mid-design for client A when B sends something urgent. Before you can start, you spend eight minutes reconstructing what you last shipped to B and what you are waiting on. You clear B, return to A, and now you have lost your own thread there.',
        lost: 'Eight minutes per switch, and the thread you were holding in A',
        after:
          'The state of each engagement is held for you: switch to B and it opens with “B — shipped the revised deck last week, waiting on their review.” Go back to A and the work picks up where you left it. The remembering step is not part of the day.',
      },
      {
        title: 'What to quote',
        before:
          'A new enquiry comes in. You have built something close before, but you cannot recall what you charged or how many days it actually took, so you spend fifteen minutes digging through old threads and quote on instinct. Halfway in, the estimate breaks.',
        lost: 'Fifteen minutes of digging, and a price based on what you actually did last time',
        after:
          'Past engagements are remembered with their numbers: “Similar landing page — quoted ¥X, actually took Y days, two over estimate.” Quoting from memory-by-instinct stops being the method.',
      },
      {
        title: 'What this client cannot stand',
        before:
          'You start a proposal for client C. Something about a casual tone went badly once, but you are not sure, so you spend ten minutes reading back through old messages to check.',
        lost: 'Ten minutes of checking, and the landmine you were about to step on again',
        after:
          'As you start writing: “C prefers a formal register — flagged your casual draft two months ago.” No search, and no second visit to the same landmine.',
      },
      {
        title: 'Terms nobody can find',
        before:
          '“We can still get another round of revisions, right?” You agreed to two at signing, but you cannot say where that was agreed, cannot produce it, and absorb the work for free.',
        lost: 'The terms you did agree, and the hours you gave away',
        after:
          'The agreement is remembered where it was made: “Two rounds of revisions, agreed in the signing email — here it is.” No excavation, and no folding because you cannot prove it.',
      },
      {
        title: 'Follow-ups that fall through',
        before:
          'Friday, 5pm. Five prospects were messaged this week. Ten minutes go into working out who replied, who did not, and who is due a nudge. One good moment passes anyway.',
        lost: 'Ten minutes of reconstruction, and the one you let go cold',
        after:
          'Sent messages and elapsed days are already tracked: “Three unanswered — one is due a follow-up today.” Nothing to reconstruct, nothing to lose.',
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
    heroEyebrow: '案件をまたぐ1日',
    heroTitle: '案件を切り替えるたび、',
    heroAccent: '頭に読み込み直す時間が消えていく',
    heroBody: 'この案件はどこまで進んだか。前回いくらで受けて何日かかったか。この人が過去に嫌がった表現は何か。思い出すために遡る時間が、手を動かす時間より長い日がある。',
    heroCta: '早期アクセスを申し込む',
    proof: ['ローカルファースト', '利用するAIを選択', '送信前に承認'],
    comparisonEyebrow: 'Before / After',
    comparisonTitle: '10%の改善ではない。10×、100×。',
    comparisonBody:
      '複数のクライアントを持つ人の1日から5つ。カードを押すと、同じ1日が ShogunAI のあとに切り替わります。',
    cases: [
      {
        title: '案件の切り替え',
        before:
          'A社のデザイン作業中にB社から急ぎの連絡。頭を切り替えるのに「B社は前回何を納品して、今は何待ちだったか」を8分思い出す。Bを片付けてAに戻ると、今度は自分がAで何をしていたか忘れている。',
        lost: '切り替えのたびに溶ける8分と、戻ったときの作業の流れ',
        after:
          '案件ごとの作業状態を保持しているので、B社に切り替えた瞬間に「B社: 先週◯◯を納品、現在は先方の確認待ち」が出る。A社に戻れば「さっきここまで作業していました」と続きが戻る。思い出す作業そのものが発生しない。',
      },
      {
        title: '見積もりの根拠',
        before:
          '新規の問い合わせに見積もりを返す。過去に似たLP制作をやったはずだが、いくらで受けて実際何日かかったか思い出せず、過去のやりとりを探して15分。結局は勘で出し、後半で工数が破綻する。',
        lost: '探し回った15分と、実績に基づいた値付け',
        after:
          '過去案件の金額と実際の所要時間を覚えているので、「類似のLP制作: 前回◯円・実働△日（見積もりより2日超過）」と実績で出る。勘で見積もる作業が消える。',
      },
      {
        title: 'クライアントの好みと地雷',
        before:
          'C社への提案文を書く。前にカジュアルすぎる文面で指摘された記憶はあるが確証がなく、過去のやりとりを遡って確認して10分。',
        lost: '確認の10分と、もう一度踏みかけた地雷',
        after:
          '書き始めた時点で「C社: フォーマルな文面を好む（2ヶ月前にカジュアルな文面で指摘あり）」が先に出る。探す作業も、地雷の再訪も起きない。',
      },
      {
        title: '契約条件の食い違い',
        before:
          'クライアントから「修正、まだできますよね?」。契約時に「修正2回まで」と決めたはずだが、どこで合意したか曖昧で、証拠を探せずに無償で対応する。',
        lost: '合意したはずの条件と、無償で出した工数',
        after:
          '契約時のやりとりを覚えているので、「契約時のメールで修正2回までと合意済み」と該当箇所を出せる。掘り返す作業も、あやふやなまま折れる事態も起きない。',
      },
      {
        title: 'フォローの取りこぼし',
        before:
          '金曜17時。今週DMした見込み客が5人。誰が返信済みで誰が未返信かをリストで見返して10分。1件、フォローの好機を逃す。',
        lost: '見返した10分と、逃した1件',
        after:
          '送信履歴と経過日数を覚えているので、「未返信3件・うち1件はそろそろフォロー時」と届く。見返す作業も、逃す事態も起きない。',
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
    heroEyebrow: 'Un día entre clientes',
    heroTitle: 'Cada cambio de cliente te cuesta el tiempo de',
    heroAccent: 'volver a cargarlo todo',
    heroBody: '¿Hasta dónde llegó este proyecto? ¿Cuánto cobraste el anterior y cuántos días llevó de verdad? ¿Qué expresión no le gustó a este cliente? Hay días en que recordar lleva más tiempo que el trabajo.',
    heroCta: 'Solicitar acceso anticipado',
    proof: ['Memoria local-first', 'Elige tu IA', 'Aprobación antes de enviar'],
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'No es un 10% mejor. Es 10×, 100×.',
    comparisonBody:
      'Cinco momentos de un día repartido entre varios clientes. Pulsa una tarjeta para cambiar hoy por el mismo día con ShogunAI.',
    cases: [
      {
        title: 'Cambiar de cliente',
        before:
          'Estás con el diseño del cliente A cuando B manda algo urgente. Antes de empezar, pasas ocho minutos reconstruyendo qué entregaste a B y qué estás esperando. Cierras B, vuelves a A y ahora has perdido tu propio hilo allí.',
        lost: 'Ocho minutos por cambio y el hilo que llevabas en A',
        after:
          'El estado de cada encargo queda guardado: al cambiar a B aparece «B: entregada la propuesta la semana pasada, pendiente de su revisión». Al volver a A, el trabajo sigue donde lo dejaste. Recordar deja de ser parte del día.',
      },
      {
        title: 'En qué basar el presupuesto',
        before:
          'Llega una consulta nueva. Hiciste algo parecido, pero no recuerdas cuánto cobraste ni cuántos días te llevó: quince minutos entre conversaciones antiguas y un presupuesto a ojo que revienta a mitad del proyecto.',
        lost: 'Quince minutos de búsqueda y un precio basado en lo que de verdad costó',
        after:
          'Los encargos anteriores se recuerdan con sus cifras: «Landing similar: cobraste X, tardaste Y días, dos más de lo previsto». Presupuestar a ojo deja de ser el método.',
      },
      {
        title: 'Lo que este cliente no tolera',
        before:
          'Empiezas una propuesta para el cliente C. Recuerdas vagamente que un tono informal salió mal, no estás seguro y dedicas diez minutos a releer mensajes antiguos.',
        lost: 'Diez minutos de comprobación y la mina que ibas a pisar otra vez',
        after:
          'Al empezar a escribir aparece: «C prefiere un tono formal: señaló tu borrador informal hace dos meses». Ni búsqueda ni segunda visita a la misma mina.',
      },
      {
        title: 'Condiciones que nadie encuentra',
        before:
          '«Todavía entra otra ronda de cambios, ¿verdad?» Acordasteis dos al firmar, pero no sabes dónde quedó por escrito, no puedes mostrarlo y asumes el trabajo gratis.',
        lost: 'Las condiciones que sí acordaste y las horas que regalaste',
        after:
          'El acuerdo se recuerda donde se hizo: «Dos rondas de revisión, acordadas en el correo de firma». Sin excavar y sin ceder por no poder demostrarlo.',
      },
      {
        title: 'Seguimientos que se caen',
        before:
          'Viernes, 17:00. Cinco posibles clientes contactados esta semana. Diez minutos para averiguar quién respondió y a quién tocaba insistir. Aun así se pasa el momento con uno.',
        lost: 'Diez minutos de reconstrucción y el contacto que se enfrió',
        after:
          'El historial y los días transcurridos ya están: «Tres sin responder, uno toca hoy». Nada que reconstruir, nada que perder.',
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
    heroEyebrow: 'Ein Tag zwischen Kunden',
    heroTitle: 'Jeder Wechsel kostet dich die Zeit,',
    heroAccent: 'alles neu zu laden',
    heroBody: 'Wie weit war dieses Projekt? Was hat das letzte gekostet und wie lange hat es wirklich gedauert? Welche Formulierung mochte dieser Kunde nicht? An manchen Tagen dauert das Erinnern länger als die Arbeit.',
    heroCta: 'Frühzugang anfragen',
    proof: ['Local-first-Gedächtnis', 'Eigene KI wählen', 'Freigabe vor dem Senden'],
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Nicht 10% besser. 10×, 100×.',
    comparisonBody:
      'Fünf Momente aus einem Tag zwischen mehreren Kunden. Tippe eine Karte an, um heute gegen denselben Tag mit ShogunAI zu tauschen.',
    cases: [
      {
        title: 'Zwischen Kunden wechseln',
        before:
          'Du bist mitten im Design für Kunde A, als B etwas Dringendes schickt. Vor dem Start gehen acht Minuten dafür drauf, zu rekonstruieren, was du B zuletzt geliefert hast und worauf du wartest. B erledigt, zurück zu A — und dort ist dein eigener Faden weg.',
        lost: 'Acht Minuten pro Wechsel und der Faden, den du in A hattest',
        after:
          'Der Stand jedes Auftrags bleibt erhalten: Beim Wechsel zu B steht da „B — letzte Woche das Konzept geliefert, wartet auf Rückmeldung“. Zurück in A läuft die Arbeit weiter. Das Erinnern gehört nicht mehr zum Tag.',
      },
      {
        title: 'Worauf ein Angebot fußt',
        before:
          'Eine neue Anfrage. Etwas Ähnliches hast du gebaut, aber weder Preis noch tatsächliche Dauer sind abrufbar: fünfzehn Minuten in alten Threads, dann ein Angebot aus dem Bauch, das zur Hälfte kippt.',
        lost: 'Fünfzehn Minuten Suche und ein Preis auf Basis dessen, was es wirklich gekostet hat',
        after:
          'Frühere Aufträge bleiben mit ihren Zahlen: „Ähnliche Landingpage — X berechnet, tatsächlich Y Tage, zwei über Schätzung.“ Aus dem Bauch schätzen ist nicht mehr die Methode.',
      },
      {
        title: 'Was dieser Kunde nicht erträgt',
        before:
          'Du beginnst ein Angebot für Kunde C. Irgendwann kam ein lockerer Ton schlecht an, sicher bist du nicht — zehn Minuten zurück durch alte Nachrichten.',
        lost: 'Zehn Minuten Prüfen und die Mine, in die du fast wieder getreten wärst',
        after:
          'Beim Schreiben erscheint: „C bevorzugt einen formellen Ton — hat deinen lockeren Entwurf vor zwei Monaten moniert.“ Keine Suche, kein zweiter Tritt in dieselbe Mine.',
      },
      {
        title: 'Konditionen, die niemand findet',
        before:
          '„Eine Korrekturrunde geht doch noch, oder?“ Bei Vertragsschluss waren es zwei, aber wo das steht, weißt du nicht, kannst es nicht zeigen und machst es umsonst.',
        lost: 'Die Konditionen, die du vereinbart hattest, und die verschenkten Stunden',
        after:
          'Die Absprache bleibt dort, wo sie getroffen wurde: „Zwei Korrekturrunden, vereinbart in der Mail zum Vertrag.“ Kein Ausgraben, kein Einknicken mangels Beleg.',
      },
      {
        title: 'Follow-ups, die durchfallen',
        before:
          'Freitag, 17 Uhr. Fünf Interessenten diese Woche angeschrieben. Zehn Minuten, um herauszufinden, wer geantwortet hat und wer eine Erinnerung braucht. Einer wird trotzdem kalt.',
        lost: 'Zehn Minuten Rekonstruktion und der eine, den du hast liegen lassen',
        after:
          'Verlauf und vergangene Tage sind erfasst: „Drei ohne Antwort — bei einem ist heute der Zeitpunkt.“ Nichts zu rekonstruieren, nichts zu verlieren.',
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

const foundersCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    ...consultantsCopyByLocale.en,
    heroEyebrow: 'A founder’s day',
    heroTitle: 'You rebuild the basis for every decision',
    heroAccent: 'from scratch each morning',
    heroBody: 'Three businesses at once, and the decisions are spread across Slack, mail and meetings. You set today’s priorities without ever seeing what changed on Friday. What is lost is not twenty-five minutes — it is the judgement you already made.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Not 10% better. 10×, 100×.',
    comparisonBody:
      'Five moments from a day spent across several businesses. Open a card to swap today for the same day with ShogunAI in it.',
    cases: [
      {
        title: 'Rebuilding the morning',
        before:
          'Slack, notes, the tabs still open from yesterday: twenty-five minutes assembling where everything stands. Each thing you recall pushes out another. You still miss that the client on business B changed terms on Friday.',
        lost: 'Twenty-five minutes rebuilt from scratch, and the change you never saw',
        after:
          'The brief is waiting when you sit down. At the top: “B — client pulled the deadline forward by a week on Friday.” Under it, today across all three businesses, in priority order. Remembering last week is not part of the morning, and where your hours should go today comes with it.',
      },
      {
        title: 'The half hour after every meeting',
        before:
          'The call ends and you write out what was decided, then a version to share: thirty minutes. And who actually said what is already blurred.',
        lost: 'Thirty minutes per meeting, and the attribution',
        after:
          'The moment the meeting ends, decisions, owners and to-dos are laid out — owners are right, because who said what was captured. It has already checked against last week: “You decided X last week; today’s conclusion contradicts it.” The email and the Slack post are drafted underneath. Read them, press a key, they are sent.',
      },
      {
        title: 'Numbers for the investor update',
        before:
          'To put figures in the deck you go back through old spreadsheets and Slack, search, copy, paste: twenty minutes. And the number you paste is last month’s, not this one.',
        lost: 'Twenty minutes of hunting, and the stale number now in the deck',
        after:
          'The figures and their latest revisions are already held, so the deck arrives with current values in it. No re-pasting. The month-on-month comparison you never had time to build comes with it.',
      },
      {
        title: 'Promises you cannot place',
        before:
          'You told three people in Slack it would be done by next week — you cannot now say which three. One is forgotten entirely until Monday, when they ask.',
        lost: 'The promises you made out loud',
        after:
          'Commitments are picked out of the conversation as they happen, so Friday reads: “Three promises made this week — one not started.” Nothing to recall, and nothing to forget.',
      },
      {
        title: 'Decisions scattered everywhere',
        before:
          '“How did we land on that?” The reasoning sits across Slack, email and meetings and cannot be seen whole. It was your own call, and the why has gone soft.',
        lost: 'The trail of your own decision-making',
        after:
          'Decisions are remembered in sequence, so the trail runs as one line. Nothing to excavate, and why the call was made is legible in one pass.',
      },
    ],
    featuresEyebrow: 'AI for the founder operating system',
    featuresTitle: 'From daily signals to decision-ready work',
    featuresBody:
      'ShogunAI connects the context behind the company, prepares the next high-stakes conversation, and helps close the loop afterward.',
    visual: {
      email: 'Product',
      meeting: 'Hiring',
      proposal: 'Customers',
      memory: 'Company memory',
      ask: 'What changed since our last investor update?',
      decisions: 'Recent metrics',
      commitments: 'Open questions',
      brief: 'Board brief ready',
      followUp: 'Investor update',
      review: 'Review required',
      approved: 'Approved',
    },
    faqTitle: 'Clear answers before company context enters memory',
    finalTitle: 'Keep the company moving without losing the why',
    finalBody: 'Connect decisions, commitments, and updates, then turn the company story into the next action.',
  },
  ja: {
    ...consultantsCopyByLocale.ja,
    heroEyebrow: '創業者の1日',
    heroTitle: '経営判断の材料を、',
    heroAccent: '毎朝ゼロから組み立て直している',
    heroBody: '3事業を並行し、決定はSlackとメールと会議に散らばる。先週の変更に気づかないまま、今日の優先順位を決めている。失われているのは25分ではなく、自分が下したはずの判断そのものだ。',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: '10%の改善ではない。10×、100×。',
    comparisonBody:
      '複数の事業を持つ人の1日から5つ。カードを押すと、同じ1日が ShogunAI のあとに切り替わります。',
    cases: [
      {
        title: '朝、状況を組み立て直す',
        before:
          'Slack、メモ、開いたままのタブをすべて開いて進捗を確認する。ひとつ思い出すと別のひとつを忘れる。25分かけても、B事業で先方が金曜に条件を変えていたことには気づかないままだった。',
        lost: '再構築した25分と、気づかなかった金曜の変更',
        after:
          '朝にはブリーフが届いている。最上部に「B事業: 金曜に先方が納期を1週間前倒し」。その下に3事業ぶんの今日やることが優先度つきで並ぶ。先週を思い出す作業が発生しない。今日どの事業に時間を割くべきかの提案まで、向こうから届く。',
      },
      {
        title: '会議のあとの30分',
        before:
          '終わるたびに決定事項を書き出し、共有文を作って30分。しかも誰がどの発言をしたのかは、あとから正確には辿れない。',
        lost: '会議のたびに戻ってこない30分と、発言の主',
        after:
          '会議が終わると同時に、決定事項・担当・ToDoが揃っている（誰の発言かを覚えているので担当が正確）。先週の決定との矛盾チェックまで済んでいて「先週はXと決めましたが、今日の結論と食い違います」と指摘が出る。関係者へのメールとSlackの共有文も下書き済みで、内容を確認してキーを押すだけで送れる。',
      },
      {
        title: '投資家向け資料の数字',
        before:
          '資料に数字を入れるため、過去のスプレッドシートとSlackを遡り、検索してコピペで20分。しかも最新ではなく古い数字を貼ってしまう。',
        lost: '探し回った20分と、資料に載った古い数値',
        after:
          '数字と最新の更新を覚えているので、最新値が入った状態で資料ができている。貼り直す作業が発生しない。時間がなくて作れなかった先月との比較まで付いてくる。',
      },
      {
        title: '誰に何を約束したか',
        before:
          'Slackで「来週までにやります」と3人に言ったはずが、誰に言ったか覚えていない。1件は完全に忘れ、月曜に相手からの連絡で発覚する。',
        lost: '自分が口にした約束そのもの',
        after:
          '会話の中の約束を拾って覚えているので、金曜の時点で「今週した約束3件、うち1件未着手」と出る。思い出す作業も、忘れること自体も起きない。',
      },
      {
        title: '散らばった意思決定',
        before:
          '「あれ、どう決めたんだっけ」。判断の経緯がSlack・メール・会議に散らばっていて、全体像が追えない。自分が下した決定なのに、なぜそうしたのかがぼやけている。',
        lost: '自分がたどってきた意思決定の履歴',
        after:
          '決定に関する時系列を覚えているので、経緯が一本につながる。掘り起こす作業が発生しない。なぜこの決定なのかを一度に把握できる。',
      },
    ],
    featuresEyebrow: '創業者の経営業務を支えるAI',
    featuresTitle: '日々の兆しを、判断できる仕事へ',
    featuresBody: '会社を形づくる経緯をつなぎ、重要な会話を準備し、その後のフォローまで同じ文脈で進めます。',
    visual: {
      email: 'プロダクト',
      meeting: '採用',
      proposal: '顧客',
      memory: '会社の記憶',
      ask: '前回の投資家更新から何が変わった？',
      decisions: '最近の指標',
      commitments: '未解決の論点',
      brief: '取締役会ブリーフ完成',
      followUp: '投資家向け更新',
      review: '確認が必要',
      approved: '承認済み',
    },
    faqTitle: '会社の文脈を記憶する前に知っておきたいこと',
    finalTitle: '会社を前へ進めても、判断理由を見失わない',
    finalBody: '判断、約束、更新をひとつの文脈につなぎ、会社の経緯を次のアクションへ変えます。',
  },
  es: {
    ...consultantsCopyByLocale.es,
    heroEyebrow: 'El día de un fundador',
    heroTitle: 'Cada mañana reconstruyes desde cero',
    heroAccent: 'la base de tus decisiones',
    heroBody: 'Tres negocios a la vez y las decisiones repartidas entre Slack, correo y reuniones. Fijas las prioridades de hoy sin haber visto lo que cambió el viernes. No se pierden veinticinco minutos: se pierde el criterio que ya aplicaste.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'No es un 10% mejor. Es 10×, 100×.',
    comparisonBody:
      'Cinco momentos de un día repartido entre varios negocios. Pulsa una tarjeta para cambiar hoy por el mismo día con ShogunAI.',
    cases: [
      {
        title: 'Reconstruir la mañana',
        before:
          'Slack, notas, las pestañas de ayer: veinticinco minutos para montar dónde está todo. Cada cosa que recuerdas expulsa otra. Y sigues sin ver que el cliente del negocio B cambió condiciones el viernes.',
        lost: 'Veinticinco minutos reconstruidos y el cambio que nunca viste',
        after:
          'El resumen te espera. Arriba: «B: el cliente adelantó una semana la entrega el viernes». Debajo, el día en los tres negocios por prioridad. Recordar la semana pasada deja de ser parte de la mañana, y llega también dónde conviene poner las horas de hoy.',
      },
      {
        title: 'La media hora posterior a cada reunión',
        before:
          'Termina la llamada y escribes lo decidido y una versión para compartir: treinta minutos. Y quién dijo qué ya está borroso.',
        lost: 'Treinta minutos por reunión y la atribución',
        after:
          'Al terminar, decisiones, responsables y tareas están listos, con los responsables correctos porque se recuerda quién dijo cada cosa. Además compara con la semana anterior: «La semana pasada decidiste X; la conclusión de hoy lo contradice». El correo y el mensaje de Slack están redactados: revisas y envías con una tecla.',
      },
      {
        title: 'Los números del informe a inversores',
        before:
          'Para meter cifras vuelves a hojas de cálculo y a Slack, buscas, copias y pegas: veinte minutos. Y el número que pegas es el del mes pasado.',
        lost: 'Veinte minutos de búsqueda y el dato caducado ya pegado',
        after:
          'Las cifras y sus últimas revisiones ya están guardadas, así que el documento sale con los valores actuales. Sin volver a pegar, y con la comparativa mensual que nunca tenías tiempo de montar.',
      },
      {
        title: 'Promesas que no logras situar',
        before:
          'Dijiste en Slack a tres personas que lo tendrías la semana que viene y ya no sabes a cuáles. Una se olvida del todo hasta que el lunes preguntan.',
        lost: 'Las promesas que hiciste en voz alta',
        after:
          'Los compromisos se detectan en la conversación, así que el viernes dice: «Tres promesas esta semana, una sin empezar». Nada que recordar y nada que olvidar.',
      },
      {
        title: 'Decisiones dispersas',
        before:
          '«¿Cómo acabamos decidiendo eso?» El razonamiento está repartido entre Slack, correo y reuniones y no se ve entero. Fue tu decisión y el porqué se ha difuminado.',
        lost: 'El rastro de tus propias decisiones',
        after:
          'Las decisiones se recuerdan en secuencia, así que el rastro se lee de una sola línea. Nada que excavar y el porqué queda claro de una pasada.',
      },
    ],
    featuresEyebrow: 'IA para el sistema operativo del fundador',
    featuresTitle: 'De señales diarias a trabajo listo para decidir',
    featuresBody:
      'ShogunAI conecta el contexto de la empresa, prepara las conversaciones importantes y ayuda a cerrar el seguimiento.',
    visual: {
      email: 'Producto',
      meeting: 'Contratación',
      proposal: 'Clientes',
      memory: 'Memoria empresarial',
      ask: '¿Qué cambió desde la última actualización a inversores?',
      decisions: 'Métricas recientes',
      commitments: 'Preguntas abiertas',
      brief: 'Briefing del consejo listo',
      followUp: 'Actualización a inversores',
      review: 'Revisión necesaria',
      approved: 'Aprobado',
    },
    faqTitle: 'Respuestas claras antes de incorporar contexto empresarial',
    finalTitle: 'Haz avanzar la empresa sin perder el porqué',
    finalBody:
      'Conecta decisiones, compromisos y actualizaciones, y convierte la historia de la empresa en la siguiente acción.',
  },
  de: {
    ...consultantsCopyByLocale.de,
    heroEyebrow: 'Der Tag einer Gründerin',
    heroTitle: 'Jeden Morgen baust du die Grundlage',
    heroAccent: 'jeder Entscheidung neu auf',
    heroBody: 'Drei Geschäfte gleichzeitig, die Entscheidungen verteilt über Slack, Mail und Meetings. Du setzt die Prioritäten für heute, ohne gesehen zu haben, was sich am Freitag geändert hat. Verloren gehen nicht 25 Minuten, sondern dein eigenes Urteil.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Nicht 10% besser. 10×, 100×.',
    comparisonBody:
      'Fünf Momente aus einem Tag zwischen mehreren Geschäften. Tippe eine Karte an, um heute gegen denselben Tag mit ShogunAI zu tauschen.',
    cases: [
      {
        title: 'Den Morgen neu zusammensetzen',
        before:
          'Slack, Notizen, die Tabs von gestern: fünfundzwanzig Minuten, um den Stand zusammenzusetzen. Jede Erinnerung verdrängt eine andere. Und dass der Kunde in Geschäft B am Freitag die Konditionen geändert hat, siehst du trotzdem nicht.',
        lost: 'Fünfundzwanzig Minuten Wiederaufbau und die Änderung, die du nie gesehen hast',
        after:
          'Das Briefing liegt bereit. Oben: „B — Kunde hat den Termin am Freitag um eine Woche vorgezogen.“ Darunter der Tag über alle drei Geschäfte, nach Priorität. Die letzte Woche zu rekonstruieren gehört nicht mehr zum Morgen — und wohin die Stunden heute gehören, kommt mit.',
      },
      {
        title: 'Die halbe Stunde nach jedem Meeting',
        before:
          'Das Gespräch endet, du schreibst Entscheidungen auf und dann eine Fassung zum Teilen: dreißig Minuten. Und wer was gesagt hat, ist schon verschwommen.',
        lost: 'Dreißig Minuten pro Meeting und die Zuordnung',
        after:
          'Sobald das Meeting endet, liegen Entscheidungen, Verantwortliche und To-dos bereit — korrekt zugeordnet, weil festgehalten ist, wer was gesagt hat. Der Abgleich mit der Vorwoche ist erledigt: „Letzte Woche hast du X entschieden; das widerspricht dem heutigen Ergebnis.“ Mail und Slack-Post stehen als Entwurf darunter: prüfen, Taste, raus.',
      },
      {
        title: 'Zahlen fürs Investoren-Update',
        before:
          'Für die Zahlen zurück in alte Tabellen und Slack, suchen, kopieren, einfügen: zwanzig Minuten. Und die eingefügte Zahl ist die vom Vormonat.',
        lost: 'Zwanzig Minuten Suche und die veraltete Zahl, die jetzt im Deck steht',
        after:
          'Zahlen und ihre letzten Änderungen sind gehalten, das Deck entsteht mit aktuellen Werten. Kein Neu-Einfügen — und der Monatsvergleich, für den nie Zeit war, kommt mit.',
      },
      {
        title: 'Zusagen, die du nicht zuordnest',
        before:
          'Du hast drei Leuten in Slack „bis nächste Woche“ zugesagt und weißt nicht mehr, welchen dreien. Eine fällt komplett aus, bis am Montag nachgefragt wird.',
        lost: 'Die Zusagen, die du ausgesprochen hast',
        after:
          'Zusagen werden im Gespräch erkannt, also steht am Freitag: „Drei Zusagen diese Woche — eine nicht begonnen.“ Nichts zu erinnern, nichts zu vergessen.',
      },
      {
        title: 'Verstreute Entscheidungen',
        before:
          '„Wie sind wir da eigentlich hingekommen?“ Die Begründung liegt über Slack, Mail und Meetings verteilt und ist nie als Ganzes zu sehen. Es war deine Entscheidung, und das Warum ist weich geworden.',
        lost: 'Die Spur deiner eigenen Entscheidungen',
        after:
          'Entscheidungen bleiben in ihrer Reihenfolge, die Spur liest sich als eine Linie. Nichts auszugraben, und das Warum ist in einem Durchgang lesbar.',
      },
    ],
    featuresEyebrow: 'KI für das Betriebssystem von Gründern',
    featuresTitle: 'Von täglichen Signalen zu entscheidungsreifer Arbeit',
    featuresBody:
      'ShogunAI verbindet den Unternehmenskontext, bereitet wichtige Gespräche vor und unterstützt das anschließende Follow-up.',
    visual: {
      email: 'Produkt',
      meeting: 'Recruiting',
      proposal: 'Kunden',
      memory: 'Unternehmensgedächtnis',
      ask: 'Was hat sich seit dem letzten Investoren-Update geändert?',
      decisions: 'Aktuelle Kennzahlen',
      commitments: 'Offene Fragen',
      brief: 'Board-Briefing bereit',
      followUp: 'Investoren-Update',
      review: 'Prüfung erforderlich',
      approved: 'Freigegeben',
    },
    faqTitle: 'Klare Antworten, bevor Unternehmenskontext gespeichert wird',
    finalTitle: 'Das Unternehmen voranbringen, ohne das Warum zu verlieren',
    finalBody:
      'Verbinde Entscheidungen, Zusagen und Updates und mache aus der Unternehmensgeschichte den nächsten Schritt.',
  },
};

const productEngineeringCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    ...consultantsCopyByLocale.en,
    heroEyebrow: 'A day of shipping',
    heroTitle: 'The “why” never lands in the code —',
    heroAccent: 'so it disappears every time',
    heroBody: 'Every interruption takes the thread with it, and three weeks later you are digging for your own reasoning. The spec sits in Slack, in a hallway conversation and in an issue, and something falls out while you collect it.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Not 10% better. 10×, 100×.',
    comparisonBody:
      'Five moments from a day spent across several codebases. Open a card to swap today for the same day with ShogunAI in it.',
    cases: [
      {
        title: 'Coming back from an interruption',
        before:
          'You are deep in a feature when a production incident takes forty minutes. Back at the editor, what you were thinking and what you meant to touch next are gone; fifteen minutes go into rebuilding it.',
        lost: 'Fifteen minutes of rebuilding, and the thread you were holding',
        after:
          'The state just before the interruption is held: you return to “you were writing error handling for this function, tests were next.” The rebuild is not part of the day.',
      },
      {
        title: 'Why the code is like that',
        before:
          'A project you have not touched in three weeks. A function is shaped oddly and you spend twenty minutes back through Slack and PR comments reconstructing why you did that.',
        lost: 'Twenty minutes of excavation, and the reasoning the code never carried',
        after:
          'The discussion and the decision behind it are remembered: open the file and “this shape avoids the external API rate limit — from Slack, three weeks ago.” The excavation stops happening.',
      },
      {
        title: 'Switching repositories',
        before:
          'Mid-feature for A when B needs an emergency fix. Opening B, ten minutes go into recalling the layout, the branch you were on and what was half-done. Back in A, the thread has snapped.',
        lost: 'Ten minutes of reloading, and the thread in A',
        after:
          'Each project keeps its state, so opening B reads “B — on the retry branch implementing the webhook, next is the migration.” Return to A and the work resumes. Nothing to load back in.',
      },
      {
        title: 'Solving the same thing twice',
        before:
          'A build error you have definitely seen before. You cannot recall which project it was or what fixed it, so it costs another hour to work out.',
        lost: 'An hour spent again, and an answer you had already found',
        after:
          'Past fixes are remembered, so the same error arrives with “solved two months ago on another project — the cause was the pinned toolchain version.” No re-deriving, and the fixes you keep scattering start accumulating in one place.',
      },
      {
        title: 'Requirements in fragments',
        before:
          'Before you can start, forty minutes go into gathering days of requirements from Slack, hallway conversations, issues and notes. One spec change from three days ago is missed, and the work is redone after the fact.',
        lost: 'Forty minutes of gathering, and the change you missed — the rework it caused',
        after:
          'Days of discussion are already held, so the spec arrives structured to the level you can build from, changes included. Nothing to gather, nothing missed, and the reasoning behind each decision stays attached.',
      },
    ],
    featuresEyebrow: 'AI across the product lifecycle',
    featuresTitle: 'From scattered decisions to delivery-ready context',
    featuresBody:
      'ShogunAI preserves the reasoning behind the work, prepares the next handoff, and helps turn project context into a reviewed artifact.',
    visual: {
      email: 'Research',
      meeting: 'Discussion',
      proposal: 'Design',
      memory: 'Product memory',
      ask: 'Why did we choose this approach?',
      decisions: 'Customer evidence',
      commitments: 'Open constraints',
      brief: 'Handoff ready',
      followUp: 'Launch update',
      review: 'Review required',
      approved: 'Approved',
    },
    faqTitle: 'Clear answers before product context enters the workflow',
    finalTitle: 'Ship the next version without losing the decisions behind it',
    finalBody:
      'Connect evidence, trade-offs, and implementation history, then turn that context into the next artifact.',
  },
  ja: {
    ...consultantsCopyByLocale.ja,
    heroEyebrow: '実装までの1日',
    heroTitle: 'コードに残らない「なぜ」が、',
    heroAccent: '毎回消えている',
    heroBody: '中断のたびに思考が飛び、3週間後の自分が理由を掘り返す。仕様はSlackと口頭とissueに散らばり、集める途中で1つ落ちて手戻りになる。',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: '10%の改善ではない。10×、100×。',
    comparisonBody:
      '複数のコードベースを行き来する人の1日から5つ。カードを押すと、同じ1日が ShogunAI のあとに切り替わります。',
    cases: [
      {
        title: '中断からの復帰',
        before:
          '機能を実装している最中に本番障害の対応が入り、40分中断。戻ると「さっき何を考えていて、次はどこを直す予定だったか」が思い出せず、立て直すのに15分かかる。',
        lost: '立て直しの15分と、中断直前の思考の流れ',
        after:
          '中断直前の作業状態を覚えているので、戻ると「この関数のエラーハンドリングを書いていて、次はテストを追加する予定でした」と流れごと戻る。立て直す作業が発生しない。',
      },
      {
        title: '設計判断の理由',
        before:
          '3週間ぶりに触るプロジェクト。ある関数が妙な作りになっていて「なぜこうした?」と当時のSlackとPRコメントを遡り、理由を再構築して20分。',
        lost: '掘り返した20分と、コードに残らなかった判断理由',
        after:
          '当時の議論と決定理由を覚えているので、その箇所を開くと「この設計にした理由: 外部APIのレート制限を回避するため（3週間前のSlackより）」が出る。理由を掘り返す作業が消える。',
      },
      {
        title: 'プロジェクト間の切り替え',
        before:
          'A社の開発中にB社の緊急対応。Bのリポジトリを開いても「この構成はどうなっていたか、今どのブランチで何をやりかけか」を思い出すのに10分。Aに戻ると作業の流れが切れている。',
        lost: 'ロードし直す10分と、切れた作業の流れ',
        after:
          '各プロジェクトの作業状態を保持しているので、Bを開いた瞬間に「Bは◯◯ブランチで△△を実装中、次は□□」が出る。Aに戻れば続きが戻る。積み直す作業が発生しない。',
      },
      {
        title: '同じ解決の再発',
        before:
          'ビルドエラーに遭遇。「これは前にも見た、解決したはず」だが、どのプロジェクトでどう直したか思い出せず、また1時間かけて調べ直す。',
        lost: '調べ直した1時間と、一度は自分が出した答え',
        after:
          '過去の解決を覚えているので、同じエラーに当たると「2ヶ月前に別プロジェクトで解決済み: ◯◯が原因」と出る。調べ直す作業が消える。散逸していた解決のナレッジが手元に積み上がる。',
      },
      {
        title: '仕様の断片の集約',
        before:
          '実装に着手する前に、Slack・口頭・issue・メモへ散らばった数日ぶんの要件を集めて仕様に整理して40分。3日前に決まった仕様変更を1つ拾い忘れ、実装後に手戻り。',
        lost: '集約の40分と、拾い忘れた仕様変更（＝手戻り）',
        after:
          '数日ぶんの議論を全部覚えているので、変更も含めて実装できる粒度の仕様に構造化済み。集める作業も、拾い忘れも起きない。決定に至った理由まで残る。',
      },
    ],
    featuresEyebrow: 'プロダクトライフサイクルを支えるAI',
    featuresTitle: '散らばった判断を、提供できる文脈へ',
    featuresBody: '仕事の背景にある理由を残し、次の引き継ぎを準備し、プロジェクト文脈を確認可能な成果物へ変えます。',
    visual: {
      email: 'リサーチ',
      meeting: '議論',
      proposal: '設計',
      memory: 'プロダクト記憶',
      ask: 'なぜこの方針を選んだ？',
      decisions: '顧客の根拠',
      commitments: '未解決の制約',
      brief: '引き継ぎ準備完了',
      followUp: 'リリース更新',
      review: '確認が必要',
      approved: '承認済み',
    },
    faqTitle: 'プロダクトの文脈を扱う前に知っておきたいこと',
    finalTitle: '判断理由を失わずに、次のバージョンを届ける',
    finalBody: '顧客の根拠、トレードオフ、実装履歴をつなぎ、その文脈を次の成果物へ変えます。',
  },
  es: {
    ...consultantsCopyByLocale.es,
    heroEyebrow: 'Un día de entregar código',
    heroTitle: 'El “porqué” no queda en el código',
    heroAccent: 'y desaparece cada vez',
    heroBody: 'Cada interrupción se lleva el hilo y tres semanas después estás desenterrando tu propio razonamiento. La especificación vive en Slack, en una conversación de pasillo y en un issue, y algo se cae mientras la reúnes.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'No es un 10% mejor. Es 10×, 100×.',
    comparisonBody:
      'Cinco momentos de un día repartido entre varias bases de código. Pulsa una tarjeta para cambiar hoy por el mismo día con ShogunAI.',
    cases: [
      {
        title: 'Volver tras una interrupción',
        before:
          'Estás dentro de una función cuando una incidencia en producción se lleva cuarenta minutos. Al volver, lo que pensabas y lo que ibas a tocar ya no está: quince minutos para rehacerlo.',
        lost: 'Quince minutos de reconstrucción y el hilo que llevabas',
        after:
          'El estado justo anterior a la interrupción queda guardado: vuelves a «estabas escribiendo el manejo de errores de esta función; lo siguiente eran los tests». La reconstrucción desaparece.',
      },
      {
        title: 'Por qué el código es así',
        before:
          'Un proyecto que no tocas desde hace tres semanas. Una función tiene una forma rara y pasas veinte minutos entre Slack y comentarios de PR reconstruyendo por qué.',
        lost: 'Veinte minutos de excavación y el motivo que el código nunca guardó',
        after:
          'La discusión y la decisión se recuerdan: abres el archivo y aparece «esta forma evita el límite de la API externa, según Slack hace tres semanas». La excavación deja de ocurrir.',
      },
      {
        title: 'Cambiar de repositorio',
        before:
          'A mitad de una función para A, B necesita un arreglo urgente. Al abrir B pierdes diez minutos recordando la estructura, la rama y qué quedó a medias. Al volver a A, el hilo se ha roto.',
        lost: 'Diez minutos de recarga y el hilo en A',
        after:
          'Cada proyecto conserva su estado: abrir B muestra «B: rama de reintentos, implementando el webhook; después, la migración». Al volver a A el trabajo sigue. Nada que recargar.',
      },
      {
        title: 'Resolver dos veces lo mismo',
        before:
          'Un error de compilación que ya has visto. No recuerdas en qué proyecto ni cómo lo arreglaste, así que se va otra hora en averiguarlo.',
        lost: 'Otra hora gastada y una respuesta que ya habías encontrado',
        after:
          'Las soluciones anteriores se recuerdan: el mismo error llega con «resuelto hace dos meses en otro proyecto: la causa era la versión fijada». Sin volver a deducirlo, y ese conocimiento disperso empieza a acumularse en un sitio.',
      },
      {
        title: 'Requisitos en fragmentos',
        before:
          'Antes de empezar, cuarenta minutos reuniendo días de requisitos de Slack, conversaciones, issues y notas. Se escapa un cambio de hace tres días y hay que rehacer trabajo.',
        lost: 'Cuarenta minutos de recopilación y el cambio que se escapó (y su retrabajo)',
        after:
          'Los días de discusión ya están guardados, así que la especificación llega estructurada al nivel con el que puedes construir, cambios incluidos. Nada que reunir, nada que se escape, y el porqué de cada decisión queda anotado.',
      },
    ],
    featuresEyebrow: 'IA para todo el ciclo de producto',
    featuresTitle: 'De decisiones dispersas a contexto listo para entregar',
    featuresBody:
      'ShogunAI conserva las razones, prepara el siguiente traspaso y convierte el contexto del proyecto en un artefacto revisado.',
    visual: {
      email: 'Investigación',
      meeting: 'Conversación',
      proposal: 'Diseño',
      memory: 'Memoria de producto',
      ask: '¿Por qué elegimos este enfoque?',
      decisions: 'Evidencia de clientes',
      commitments: 'Restricciones abiertas',
      brief: 'Traspaso listo',
      followUp: 'Actualización de lanzamiento',
      review: 'Revisión necesaria',
      approved: 'Aprobado',
    },
    faqTitle: 'Respuestas claras antes de incorporar contexto de producto',
    finalTitle: 'Entrega la siguiente versión sin perder las decisiones anteriores',
    finalBody:
      'Conecta evidencia, compromisos e historial de implementación y convierte ese contexto en el siguiente artefacto.',
  },
  de: {
    ...consultantsCopyByLocale.de,
    heroEyebrow: 'Ein Tag im Ausliefern',
    heroTitle: 'Das „Warum“ landet nie im Code —',
    heroAccent: 'und verschwindet jedes Mal',
    heroBody: 'Jede Unterbrechung nimmt den Faden mit, und drei Wochen später gräbst du nach deiner eigenen Begründung. Die Spezifikation steckt in Slack, in einem Flurgespräch und in einem Issue — beim Einsammeln fällt etwas heraus.',
    comparisonEyebrow: 'Before / After',
    comparisonTitle: 'Nicht 10% besser. 10×, 100×.',
    comparisonBody:
      'Fünf Momente aus einem Tag zwischen mehreren Codebasen. Tippe eine Karte an, um heute gegen denselben Tag mit ShogunAI zu tauschen.',
    cases: [
      {
        title: 'Rückkehr nach einer Unterbrechung',
        before:
          'Du steckst in einem Feature, als ein Produktionsvorfall vierzig Minuten kostet. Zurück im Editor sind Gedanke und nächster Schritt weg; fünfzehn Minuten gehen in den Wiederaufbau.',
        lost: 'Fünfzehn Minuten Wiederaufbau und der Faden, den du hattest',
        after:
          'Der Stand direkt vor der Unterbrechung bleibt: Du kommst zurück zu „du hast das Error-Handling dieser Funktion geschrieben, als Nächstes standen Tests an“. Der Wiederaufbau entfällt.',
      },
      {
        title: 'Warum der Code so aussieht',
        before:
          'Ein Projekt, das du drei Wochen nicht angefasst hast. Eine Funktion ist seltsam gebaut, und zwanzig Minuten gehen durch Slack und PR-Kommentare, um den Grund zu rekonstruieren.',
        lost: 'Zwanzig Minuten Ausgraben und die Begründung, die der Code nie trug',
        after:
          'Diskussion und Entscheidung bleiben erhalten: Beim Öffnen steht „diese Form umgeht das Rate-Limit der externen API — aus Slack, vor drei Wochen“. Das Ausgraben hört auf.',
      },
      {
        title: 'Repository wechseln',
        before:
          'Mitten im Feature für A braucht B einen Notfall-Fix. Beim Öffnen von B gehen zehn Minuten für Struktur, Branch und halb fertige Arbeit drauf. Zurück in A ist der Faden gerissen.',
        lost: 'Zehn Minuten Nachladen und der Faden in A',
        after:
          'Jedes Projekt behält seinen Stand: B öffnet mit „B — Retry-Branch, Webhook in Arbeit, danach die Migration“. Zurück in A läuft es weiter. Nichts nachzuladen.',
      },
      {
        title: 'Dasselbe zweimal lösen',
        before:
          'Ein Build-Fehler, den du sicher schon hattest. In welchem Projekt und wie behoben, weißt du nicht — also noch eine Stunde.',
        lost: 'Eine weitere Stunde und eine Antwort, die du schon hattest',
        after:
          'Frühere Lösungen bleiben: Derselbe Fehler kommt mit „vor zwei Monaten in einem anderen Projekt gelöst — Ursache war die gepinnte Version“. Kein erneutes Herleiten, und das verstreute Wissen sammelt sich an einer Stelle.',
      },
      {
        title: 'Anforderungen in Fragmenten',
        before:
          'Vor dem Start vierzig Minuten, um tagelange Anforderungen aus Slack, Gesprächen, Issues und Notizen einzusammeln. Eine Änderung von vor drei Tagen fehlt, danach Nacharbeit.',
        lost: 'Vierzig Minuten Sammeln und die übersehene Änderung (samt Nacharbeit)',
        after:
          'Die Diskussionen mehrerer Tage sind gehalten, die Spezifikation liegt umsetzbar strukturiert vor, Änderungen inklusive. Nichts einzusammeln, nichts zu übersehen — und die Begründung bleibt an jeder Entscheidung.',
      },
    ],
    featuresEyebrow: 'KI über den gesamten Produktlebenszyklus',
    featuresTitle: 'Von verstreuten Entscheidungen zu lieferbereitem Kontext',
    featuresBody:
      'ShogunAI bewahrt die Gründe hinter der Arbeit, bereitet die nächste Übergabe vor und macht aus Projektkontext ein geprüftes Artefakt.',
    visual: {
      email: 'Recherche',
      meeting: 'Diskussion',
      proposal: 'Design',
      memory: 'Produktgedächtnis',
      ask: 'Warum haben wir diesen Ansatz gewählt?',
      decisions: 'Kundensignale',
      commitments: 'Offene Beschränkungen',
      brief: 'Übergabe bereit',
      followUp: 'Launch-Update',
      review: 'Prüfung erforderlich',
      approved: 'Freigegeben',
    },
    faqTitle: 'Klare Antworten, bevor Produktkontext in den Workflow gelangt',
    finalTitle: 'Die nächste Version liefern, ohne frühere Entscheidungen zu verlieren',
    finalBody: 'Verbinde Signale, Abwägungen und Umsetzungshistorie und mache daraus das nächste Artefakt.',
  },
};

const copyBySlug = {
  founders: foundersCopyByLocale,
  'product-engineering': productEngineeringCopyByLocale,
  consultants: consultantsCopyByLocale,
} as const;

type UseCaseSlug = keyof typeof copyBySlug;


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
      <div className="absolute top-1/2 left-1/2 flex size-[104px] -translate-x-1/2 -translate-y-1/2 flex-col items-center justify-center rounded-full border border-[#b991ff]/45 bg-[radial-gradient(circle,#733cff,#321d69)] shadow-[0_0_54px_rgba(130,73,255,0.48)]">
        <Sparkles className="size-6" aria-hidden="true" />
        <span className="mt-1 max-w-[80px] text-center text-[11px] leading-tight font-semibold">{copy.memory}</span>
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
          <div key={label} className="flex items-center gap-3 rounded-xl border border-white/8 bg-white/[0.035] p-3">
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

const caseUi: Record<Locale, CaseLabels> = {
  en: {
    before: 'Before',
    after: 'After',
    lost: 'What it costs:',
    seeAfter: 'See the same day with ShogunAI',
    seeBefore: 'Back to today',
    region: 'Before / After cases',
    prev: 'Previous case',
    next: 'Next case',
  },
  ja: {
    before: 'Before',
    after: 'After',
    lost: '失われるもの:',
    seeAfter: 'After（ShogunAI のあと）を見る',
    seeBefore: 'Before に戻る',
    region: 'Before / After の事例',
    prev: '前の事例',
    next: '次の事例',
  },
  es: {
    before: 'Before',
    after: 'After',
    lost: 'Lo que cuesta:',
    seeAfter: 'Ver el mismo día con ShogunAI',
    seeBefore: 'Volver a hoy',
    region: 'Casos Before / After',
    prev: 'Caso anterior',
    next: 'Caso siguiente',
  },
  de: {
    before: 'Before',
    after: 'After',
    lost: 'Was es kostet:',
    seeAfter: 'Denselben Tag mit ShogunAI ansehen',
    seeBefore: 'Zurück zu heute',
    region: 'Before / After — Fälle',
    prev: 'Vorheriger Fall',
    next: 'Nächster Fall',
  },
};

export function isProductLedUseCase(slug: string): slug is UseCaseSlug {
  return slug in copyBySlug;
}

export function UseCaseMarketingPage({ page, locale }: { page: MarketingDetail; locale: Locale }) {
  if (!isProductLedUseCase(page.slug)) return null;

  const copy = copyBySlug[page.slug][locale];
  const ui = caseUi[locale];
  const homeCta = `/${locale}/#get-started`;
  const featureVisuals = [MemoryVisual, BriefVisual, FollowUpVisual];

  return (
    <div className="bg-bg overflow-hidden">
      <header className="border-border bg-bg relative border-b py-[clamp(68px,8vw,118px)]">
        <div
          className="absolute inset-x-0 top-0 h-[420px] bg-[radial-gradient(ellipse_at_70%_0%,rgba(110,82,255,0.12),transparent_55%)]"
          aria-hidden="true"
        />
        <div className="container-x relative">
          <div className="max-w-[900px]">
            <p className="text-xs font-semibold tracking-[0.15em] text-[#6758ff] uppercase">{copy.heroEyebrow}</p>
            <h1 className="text-ink mt-6 max-w-[19ch] font-sans text-[clamp(44px,5.4vw,72px)] leading-[1.02] font-semibold tracking-[-0.055em] text-balance">
              {copy.heroTitle}{' '}
              <span className="inline-block bg-[linear-gradient(95deg,#25252b_5%,#8b8b94_65%,#b0b0b8)] bg-clip-text text-transparent [html[data-theme='dark']_&]:bg-[linear-gradient(95deg,#ffffff_5%,#a8a8b5_75%)] [html[data-theme='dark']_&]:bg-clip-text">
                {copy.heroAccent}
              </span>
            </h1>
            <p className="text-muted mt-7 max-w-[680px] text-[clamp(17px,1.55vw,21px)] leading-[1.65]">
              {copy.heroBody}
            </p>
            <Link
              href={homeCta}
              className="bg-ink text-bg mt-8 inline-flex min-h-14 items-center justify-center gap-3 rounded-[14px] px-7 text-[15px] font-semibold shadow-[0_14px_32px_rgba(18,18,23,0.16)] transition-transform hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-[#6758ff] focus-visible:ring-offset-4 focus-visible:outline-none"
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
        </div>
      </header>

      <section className="py-[clamp(72px,9vw,126px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[940px] text-center">
            <p className="text-xs font-semibold tracking-[0.15em] text-[#6758ff] uppercase">{copy.comparisonEyebrow}</p>
            <h2 className="text-ink mt-5 font-sans text-[clamp(38px,5.3vw,68px)] leading-[1.06] font-semibold tracking-[-0.06em] text-balance [word-break:keep-all]">
              {copy.comparisonTitle}
            </h2>
            <p className="text-muted mx-auto mt-6 max-w-[760px] text-[clamp(16px,1.35vw,19px)] leading-[1.65]">
              {copy.comparisonBody}
            </p>
          </div>
          <CaseCards cases={copy.cases} labels={ui} />
          <div className="mt-12 flex justify-center">
            <Link
              href={homeCta}
              className="bg-ink text-bg inline-flex min-h-13 items-center gap-2 rounded-xl px-6 py-3.5 text-sm font-semibold"
            >
              {copy.heroCta}
              <ArrowRight className="size-4" />
            </Link>
          </div>
        </div>
      </section>

      <section className="border-border bg-cloud/35 border-y py-[clamp(72px,9vw,122px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[920px] text-center">
            <p className="text-xs font-semibold tracking-[0.15em] text-[#6758ff] uppercase">{copy.featuresEyebrow}</p>
            <h2 className="text-ink mt-5 font-sans text-[clamp(38px,5.2vw,66px)] leading-[1] font-semibold tracking-[-0.06em] text-balance">
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
                  <div className="px-2 pt-7 pb-3">
                    <h3 className="text-ink text-[clamp(22px,2vw,29px)] leading-[1.08] font-semibold tracking-[-0.04em]">
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

      <section className="border-border bg-cloud/45 border-y py-[clamp(64px,8vw,104px)]">
        <div className="container-x max-w-[860px]">
          <p className="text-center text-xs font-semibold tracking-[0.15em] text-[#6758ff] uppercase">
            {copy.faqEyebrow}
          </p>
          <h2 className="text-ink mx-auto mt-5 max-w-[17ch] text-center font-sans text-[clamp(36px,4.5vw,56px)] leading-[1.02] font-semibold tracking-[-0.055em] text-balance">
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
              <h2 className="font-sans text-[clamp(42px,6vw,74px)] leading-[0.97] font-semibold tracking-[-0.065em] text-balance">
                {copy.finalTitle}
              </h2>
              <p className="mx-auto mt-7 max-w-[680px] text-[clamp(17px,1.7vw,21px)] leading-[1.6] text-white/85">
                {copy.finalBody}
              </p>
              <Link
                href={homeCta}
                className="mt-9 inline-flex min-h-14 items-center justify-center gap-3 rounded-[14px] bg-white px-8 text-[15px] font-semibold text-[#07131f] transition-transform hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-white focus-visible:ring-offset-4 focus-visible:ring-offset-[#08354f] focus-visible:outline-none"
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
