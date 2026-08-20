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
import { CTA } from '@/components/sections/CTA';
import { Button } from '@/components/ui/button';
import type { Dictionary } from '@/i18n/dictionaries';
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
type UseCaseCopy = {
  heroEyebrow: string;
  heroTitle: string;
  heroAccent: string;
  heroCta: string;
  proof: readonly [string, string, string];
  comparisonEyebrow: string;
  comparisonTitle: string;
  comparisonBody: string;
  oldTitle: string;
  newTitle: string;
  oldItems: readonly [string, string, string, string];
  newItems: readonly [string, string, string, string];
  featuresEyebrow: string;
  featuresTitle: string;
  featuresBody: string;
  visual: VisualCopy;
  faqEyebrow: string;
  faqTitle: string;
};

const consultantsCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    heroEyebrow: 'Built for client work',
    heroTitle: 'Every client context, in one',
    heroAccent: 'private memory',
    heroCta: 'Get early access',
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
    featuresEyebrow: 'From memory to delivery',
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
  },
  ja: {
    heroEyebrow: '顧客業務のために',
    heroTitle: 'すべての顧客文脈を、ひとつの',
    heroAccent: 'プライベートな記憶へ',
    heroCta: '早期アクセスを申し込む',
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
    featuresEyebrow: '記憶から成果物へ',
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
  },
  es: {
    heroEyebrow: 'Hecho para el trabajo con clientes',
    heroTitle: 'Todo el contexto de tus clientes, en una',
    heroAccent: 'memoria privada',
    heroCta: 'Solicitar acceso anticipado',
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
    featuresEyebrow: 'De la memoria a la entrega',
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
  },
  de: {
    heroEyebrow: 'Für Kundenarbeit entwickelt',
    heroTitle: 'Jeder Kundenkontext in einem',
    heroAccent: 'privaten Gedächtnis',
    heroCta: 'Frühzugang anfragen',
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
    featuresEyebrow: 'Vom Gedächtnis zur Lieferung',
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
  },
};

const foundersCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    ...consultantsCopyByLocale.en,
    heroEyebrow: 'Built for founders',
    heroTitle: 'Every company decision, in one',
    heroAccent: 'private memory',
    comparisonTitle: 'Company context is fragmented. Lead with the full picture.',
    comparisonBody:
      'Connect product, hiring, customers, fundraising, and operations so the next decision starts from what the company already knows.',
    oldTitle: 'The old way',
    newTitle: 'The ShogunAI way',
    oldItems: [
      'Search across product, hiring, customer, and fundraising tools',
      'Rebuild the company story before every investor or board update',
      'Lose decision rationale after only the final document remains',
      'Carry commitments across every role in your head',
    ],
    newItems: [
      'Recall company context from your private work memory',
      'Prepare investor and board briefings from context you already have',
      'Recover the reasoning behind a decision before making the next one',
      'Draft updates and review them before they leave your control',
    ],
    featuresEyebrow: 'From memory to the next decision',
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
  },
  ja: {
    ...consultantsCopyByLocale.ja,
    heroEyebrow: '創業者のために',
    heroTitle: 'すべての経営判断を、ひとつの',
    heroAccent: 'プライベートな記憶へ',
    comparisonTitle: '分散した会社の文脈を、次の判断につなぐ。',
    comparisonBody:
      'プロダクト、採用、顧客、資金調達、業務の経緯をつなぎ、会社がすでに知っていることから次の判断を始めます。',
    oldTitle: 'これまでの経営業務',
    newTitle: 'ShogunAIなら',
    oldItems: [
      'プロダクト、採用、顧客、資金調達の情報を横断して探す',
      '投資家・取締役会向けの説明を毎回ゼロから組み立てる',
      '最終資料だけが残り、判断理由が失われる',
      '役割を切り替えながら約束事項を頭の中で抱える',
    ],
    newItems: [
      'プライベートな仕事の記憶から会社の文脈を呼び出す',
      '既存の経緯から投資家・取締役会ブリーフを準備する',
      '次の判断前に、過去の理由と前提を取り戻す',
      '更新を下書きし、外部へ出る前に確認する',
    ],
    featuresEyebrow: '記憶から、次の判断へ',
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
  },
  es: {
    ...consultantsCopyByLocale.es,
    heroEyebrow: 'Hecho para fundadores',
    heroTitle: 'Cada decisión de la empresa, en una',
    heroAccent: 'memoria privada',
    comparisonTitle: 'El contexto de la empresa está fragmentado. Lidera con la imagen completa.',
    comparisonBody:
      'Conecta producto, contratación, clientes, financiación y operaciones para empezar cada decisión con lo que la empresa ya sabe.',
    oldTitle: 'La forma anterior',
    newTitle: 'La forma ShogunAI',
    oldItems: [
      'Buscar entre herramientas de producto, contratación, clientes y financiación',
      'Reconstruir la historia antes de cada actualización para inversores o consejo',
      'Perder las razones de una decisión cuando solo queda el documento final',
      'Guardar en tu cabeza compromisos de todas tus funciones',
    ],
    newItems: [
      'Recuperar contexto empresarial desde tu memoria privada de trabajo',
      'Preparar briefings para inversores y consejo con el contexto existente',
      'Recuperar las razones de una decisión antes de tomar la siguiente',
      'Redactar actualizaciones y revisarlas antes de compartirlas',
    ],
    featuresEyebrow: 'De la memoria a la siguiente decisión',
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
  },
  de: {
    ...consultantsCopyByLocale.de,
    heroEyebrow: 'Für Gründer entwickelt',
    heroTitle: 'Jede Unternehmensentscheidung in einem',
    heroAccent: 'privaten Gedächtnis',
    comparisonTitle: 'Unternehmenskontext ist fragmentiert. Führe mit dem Gesamtbild.',
    comparisonBody:
      'Verbinde Produkt, Recruiting, Kunden, Finanzierung und Betrieb, damit jede Entscheidung mit dem vorhandenen Wissen beginnt.',
    oldTitle: 'Die bisherige Arbeitsweise',
    newTitle: 'Die ShogunAI-Arbeitsweise',
    oldItems: [
      'In Produkt-, Recruiting-, Kunden- und Finanzierungstools suchen',
      'Die Unternehmensgeschichte vor jedem Investoren- oder Board-Update neu aufbauen',
      'Entscheidungsgründe verlieren, sobald nur das Enddokument bleibt',
      'Zusagen aus allen Rollen im Kopf behalten',
    ],
    newItems: [
      'Unternehmenskontext aus dem privaten Arbeitsgedächtnis abrufen',
      'Investoren- und Board-Briefings aus vorhandenem Kontext vorbereiten',
      'Vor der nächsten Entscheidung die bisherigen Gründe zurückholen',
      'Updates entwerfen und vor dem Teilen prüfen',
    ],
    featuresEyebrow: 'Vom Gedächtnis zur nächsten Entscheidung',
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
  },
};

const productEngineeringCopyByLocale: Record<Locale, UseCaseCopy> = {
  en: {
    ...consultantsCopyByLocale.en,
    heroEyebrow: 'Built for product work',
    heroTitle: 'Every product decision, in one',
    heroAccent: 'private memory',
    comparisonTitle: 'Product work is fragmented. Connect decision to delivery.',
    comparisonBody:
      'Bring customer evidence, discussion, design, and implementation into one private context layer that carries the why forward.',
    oldTitle: 'The old way',
    newTitle: 'The ShogunAI way',
    oldItems: [
      'Search across chat, documents, design files, and issue trackers',
      'Lose the rationale once a decision becomes a ticket',
      'Rebuild context manually for every brief and handoff',
      'Restart the project story after every interruption',
    ],
    newItems: [
      'Recall decisions from your private project memory',
      'Connect customer evidence, design constraints, and implementation history',
      'Prepare briefs and handoffs from context you already have',
      'Draft updates and review consequential actions before they run',
    ],
    featuresEyebrow: 'From memory to shipped work',
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
  },
  ja: {
    ...consultantsCopyByLocale.ja,
    heroEyebrow: 'プロダクト業務のために',
    heroTitle: 'すべてのプロダクト判断を、ひとつの',
    heroAccent: 'プライベートな記憶へ',
    comparisonTitle: '分断したプロダクト業務を、判断から提供までつなぐ。',
    comparisonBody: '顧客の声、議論、設計、実装をひとつのプライベートな文脈につなぎ、判断理由を次の工程へ運びます。',
    oldTitle: 'これまでのプロダクト業務',
    newTitle: 'ShogunAIなら',
    oldItems: [
      'チャット、文書、デザイン、課題管理を横断して探す',
      '判断がチケットになると、その理由が失われる',
      '仕様書や引き継ぎのたびに文脈を手作業で組み立てる',
      '中断から戻るたびにプロジェクトの経緯をたどり直す',
    ],
    newItems: [
      'プライベートなプロジェクト記憶から判断を呼び出す',
      '顧客の根拠、設計上の制約、実装履歴をつなぐ',
      '既存の文脈から仕様書や引き継ぎを準備する',
      '更新を下書きし、重要操作は実行前に確認する',
    ],
    featuresEyebrow: '記憶から、出荷まで',
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
  },
  es: {
    ...consultantsCopyByLocale.es,
    heroEyebrow: 'Hecho para el trabajo de producto',
    heroTitle: 'Cada decisión de producto, en una',
    heroAccent: 'memoria privada',
    comparisonTitle: 'El trabajo de producto está fragmentado. Conecta decisión y entrega.',
    comparisonBody:
      'Une evidencia de clientes, conversaciones, diseño e implementación en una capa privada que conserva el porqué.',
    oldTitle: 'La forma anterior',
    newTitle: 'La forma ShogunAI',
    oldItems: [
      'Buscar entre chat, documentos, diseños y gestores de incidencias',
      'Perder las razones cuando una decisión se convierte en una tarea',
      'Reconstruir contexto manualmente para cada briefing y traspaso',
      'Reiniciar la historia del proyecto después de cada interrupción',
    ],
    newItems: [
      'Recuperar decisiones desde tu memoria privada de proyecto',
      'Conectar evidencia de clientes, restricciones de diseño e historial de implementación',
      'Preparar briefings y traspasos con el contexto existente',
      'Redactar actualizaciones y revisar acciones importantes antes de ejecutarlas',
    ],
    featuresEyebrow: 'De la memoria al trabajo entregado',
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
  },
  de: {
    ...consultantsCopyByLocale.de,
    heroEyebrow: 'Für Produktarbeit entwickelt',
    heroTitle: 'Jede Produktentscheidung in einem',
    heroAccent: 'privaten Gedächtnis',
    comparisonTitle: 'Produktarbeit ist fragmentiert. Verbinde Entscheidung und Auslieferung.',
    comparisonBody:
      'Führe Kundensignale, Diskussion, Design und Umsetzung in einer privaten Kontextebene zusammen, die das Warum weiterträgt.',
    oldTitle: 'Die bisherige Arbeitsweise',
    newTitle: 'Die ShogunAI-Arbeitsweise',
    oldItems: [
      'In Chat, Dokumenten, Design-Dateien und Issue-Trackern suchen',
      'Die Begründung verlieren, sobald eine Entscheidung zum Ticket wird',
      'Kontext für jedes Briefing und jede Übergabe manuell neu aufbauen',
      'Nach jeder Unterbrechung die Projektgeschichte neu beginnen',
    ],
    newItems: [
      'Entscheidungen aus dem privaten Projektgedächtnis abrufen',
      'Kundensignale, Designbeschränkungen und Umsetzungshistorie verbinden',
      'Briefings und Übergaben aus vorhandenem Kontext vorbereiten',
      'Updates entwerfen und folgenreiche Aktionen vor der Ausführung prüfen',
    ],
    featuresEyebrow: 'Vom Gedächtnis zur ausgelieferten Arbeit',
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
    { label: copy.email, Icon: Mail, color: 'text-[#7fa6ff]' },
    { label: copy.meeting, Icon: Video, color: 'text-[#a9bdff]' },
    { label: copy.proposal, Icon: FileText, color: 'text-[#d6e0ff]' },
  ];
  return (
    <div className="relative flex h-[250px] items-center justify-center overflow-hidden rounded-[20px] bg-[#090a0f] p-5 text-white">
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_50%_55%,rgba(0,76,252,0.36),transparent_38%)]" />
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
      <div className="absolute top-1/2 left-1/2 flex size-[104px] -translate-x-1/2 -translate-y-1/2 flex-col items-center justify-center rounded-full border border-[#8fb0ff]/45 bg-[radial-gradient(circle,#2c62ff,#0d1f5c)] shadow-[0_0_54px_rgba(0,76,252,0.45)]">
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
        <Search className="size-4 shrink-0 text-[#8fb0ff]" aria-hidden="true" />
        <span className="truncate text-[11px] text-white/60">{copy.ask}</span>
        <Sparkles className="ml-auto size-4 shrink-0 text-[#a9bdff]" aria-hidden="true" />
      </div>
      <div className="mt-4 grid gap-2">
        {[copy.decisions, copy.commitments].map((label, index) => (
          <div key={label} className="flex items-center gap-3 rounded-xl border border-white/8 bg-white/[0.035] p-3">
            <span className={`size-2 rounded-full ${index === 0 ? 'bg-[#4a7bff]' : 'bg-[#45c978]'}`} />
            <span className="text-[11px] text-white/70">{label}</span>
            <span className="ml-auto h-1.5 w-12 rounded-full bg-white/10" />
          </div>
        ))}
      </div>
      <div className="mt-4 flex items-center gap-2 rounded-xl bg-[linear-gradient(110deg,#1e46c8,#004cfc)] px-4 py-3 text-[12px] font-semibold shadow-[0_14px_35px_rgba(0,60,200,0.3)]">
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
          <span className="flex size-9 items-center justify-center rounded-xl bg-[#2c56e8]">
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

export function isProductLedUseCase(slug: string): slug is UseCaseSlug {
  return slug in copyBySlug;
}

export function UseCaseMarketingPage({
  page,
  locale,
  t,
  section,
  sectionLabel,
  exploreLabel,
  exploreSub,
  overviewLabel,
}: {
  page: MarketingDetail;
  locale: Locale;
  t: Dictionary;
  section: string;
  sectionLabel: string;
  exploreLabel: string;
  exploreSub: string;
  overviewLabel: string;
}) {
  if (!isProductLedUseCase(page.slug)) return null;

  const copy = copyBySlug[page.slug][locale];
  const homeCta = `/${locale}/#get-started`;
  const featureVisuals = [MemoryVisual, BriefVisual, FollowUpVisual];

  return (
    <div className="bg-bg overflow-hidden">
      <header className="border-border bg-bg relative border-b py-[clamp(68px,8vw,118px)]">
        <div
          className="absolute inset-x-0 top-0 h-[420px] bg-[radial-gradient(ellipse_at_70%_0%,rgba(0,76,252,0.10),transparent_55%)]"
          aria-hidden="true"
        />
        <div className="container-x relative">
          <div className="max-w-[900px]">
            <p className="text-accent text-xs font-semibold tracking-[0.08em] uppercase">{copy.heroEyebrow}</p>
            <h1 className="font-display text-ink mt-5 max-w-[24ch] text-[clamp(30px,4.5vw,48px)] leading-[1.08] font-semibold tracking-[-0.02em] text-balance">
              {copy.heroTitle} <span className="text-accent">{copy.heroAccent}</span>
            </h1>
            <p className="text-muted mt-7 max-w-[640px] text-[clamp(17px,1.55vw,21px)] leading-[1.6]">
              {page.description}
            </p>
            <Button asChild className="mt-8">
              <Link href={homeCta}>
                {copy.heroCta}
                <ArrowRight className="size-4" />
              </Link>
            </Button>
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
            <p className="text-accent text-xs font-semibold tracking-[0.08em] uppercase">{copy.comparisonEyebrow}</p>
            <h2 className="font-display text-ink mt-3 text-[clamp(26px,4vw,40px)] leading-tight font-semibold text-balance">
              {copy.comparisonTitle}
            </h2>
            <p className="text-muted mx-auto mt-6 max-w-[760px] text-[clamp(16px,1.35vw,19px)] leading-[1.65]">
              {copy.comparisonBody}
            </p>
          </div>
          <div className="theme-light-panel border-border bg-surface mt-14 grid overflow-hidden rounded-[26px] border lg:grid-cols-2">
            <article className="lg:border-border p-[clamp(28px,4.5vw,58px)] lg:border-r">
              <h3 className="font-display text-muted text-[clamp(20px,2vw,26px)] leading-tight font-semibold">
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
            <article className="bg-sky-soft p-[clamp(28px,4.5vw,58px)]">
              <h3 className="font-display text-ink text-[clamp(20px,2vw,26px)] leading-tight font-semibold">
                {copy.newTitle}
              </h3>
              <ul className="mt-8 grid gap-5">
                {copy.newItems.map((item) => (
                  <li
                    key={item}
                    className="text-ink flex items-start gap-4 text-[clamp(15px,1.25vw,18px)] leading-[1.55] font-medium"
                  >
                    <Check className="mt-1 size-5 shrink-0 text-[#25a65a]" strokeWidth={2.8} aria-hidden="true" />
                    {item}
                  </li>
                ))}
              </ul>
              <Button asChild size="sm" className="mt-9">
                <Link href={homeCta}>
                  {copy.heroCta}
                  <ArrowRight className="size-4" />
                </Link>
              </Button>
            </article>
          </div>
        </div>
      </section>

      <section className="border-border bg-cloud/35 border-y py-[clamp(72px,9vw,122px)]">
        <div className="container-x">
          <div className="mx-auto max-w-[920px] text-center">
            <p className="text-accent text-xs font-semibold tracking-[0.08em] uppercase">{copy.featuresEyebrow}</p>
            <h2 className="font-display text-ink mt-3 text-[clamp(26px,4vw,40px)] leading-tight font-semibold text-balance">
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
                    <h3 className="font-display text-ink text-[clamp(20px,2vw,26px)] leading-tight font-semibold text-balance">
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
          <p className="text-center text-accent text-xs font-semibold tracking-[0.08em] uppercase">
            {copy.faqEyebrow}
          </p>
          <h2 className="font-display text-ink mx-auto mt-3 max-w-[24ch] text-center text-[clamp(26px,4vw,40px)] leading-tight font-semibold text-balance">
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

      <section className="py-14">
        <div className="container-x border-border bg-surface flex flex-col items-center justify-between gap-5 rounded-[24px] border p-7 text-center shadow-[var(--shadow-card)] sm:flex-row sm:text-left">
          <div>
            <p className="font-display text-xl font-semibold">
              {exploreLabel} {sectionLabel}
            </p>
            <p className="text-muted mt-1 text-sm">{exploreSub}</p>
          </div>
          <Button asChild variant="secondary">
            <Link href={`/${locale}/${section}`}>
              {overviewLabel} <ArrowRight className="size-4" />
            </Link>
          </Button>
        </div>
      </section>

      <CTA t={t} />
    </div>
  );
}
