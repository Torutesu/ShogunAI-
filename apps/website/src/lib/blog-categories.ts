import type { Locale } from '@/i18n/config';

export const BLOG_CATEGORY_SLUGS = ['ai-memory', 'work-context', 'comparisons', 'privacy', 'product'] as const;

export type BlogCategorySlug = (typeof BLOG_CATEGORY_SLUGS)[number];

type CategoryCopy = {
  key: string;
  title: string;
  description: string;
};

const CATEGORY_COPY: Record<Locale, Record<BlogCategorySlug, CategoryCopy>> = {
  en: {
    'ai-memory': {
      key: 'AI Memory',
      title: 'AI memory for knowledge work',
      description: 'Guides to AI memory, context, recall, and the systems that help knowledge workers pick up where they left off.',
    },
    'work-context': {
      key: 'Work Context',
      title: 'Work context across your tools',
      description: 'Practical ideas for capturing and searching the decisions, conversations, and documents spread across your workday.',
    },
    comparisons: {
      key: 'Comparisons',
      title: 'AI memory tool comparisons',
      description: 'Clear comparisons between ShogunAI and the tools people use for notes, search, memory, and team knowledge.',
    },
    privacy: {
      key: 'Privacy',
      title: 'Private and local-first AI',
      description: 'How local-first memory changes the way personal work context is captured, stored, and used.',
    },
    product: {
      key: 'Product',
      title: 'ShogunAI product notes',
      description: 'Product thinking, release notes, and field notes from building a private AI memory layer.',
    },
  },
  ja: {
    'ai-memory': {
      key: 'AI Memory',
      title: 'ナレッジワークのためのAIメモリ',
      description: 'AIメモリ、文脈検索、想起、中断した仕事をすぐに再開するための仕組みを解説します。',
    },
    'work-context': {
      key: 'Work Context',
      title: 'ツールをまたぐ仕事の文脈',
      description: '一日の仕事に散らばる意思決定、会話、文書を記録し、検索するための実践的なアイデアを紹介します。',
    },
    comparisons: {
      key: 'Comparisons',
      title: 'AIメモリツールの比較',
      description: 'ShogunAIと、メモ・検索・AIメモリ・チームナレッジの主要ツールをわかりやすく比較します。',
    },
    privacy: {
      key: 'Privacy',
      title: 'プライベートなローカルファーストAI',
      description: 'ローカルファーストなメモリが、個人の仕事の文脈の取得・保存・活用をどう変えるかを解説します。',
    },
    product: {
      key: 'Product',
      title: 'ShogunAIプロダクトノート',
      description: 'プライベートなAIメモリレイヤーをつくる過程のプロダクト思想、リリース、開発ノートを届けます。',
    },
  },
  es: {
    'ai-memory': {
      key: 'AI Memory',
      title: 'Memoria de IA para el trabajo del conocimiento',
      description: 'Guías sobre memoria de IA, contexto, recuperación y sistemas para retomar el trabajo donde lo dejaste.',
    },
    'work-context': {
      key: 'Work Context',
      title: 'Contexto de trabajo entre tus herramientas',
      description: 'Ideas prácticas para capturar y buscar decisiones, conversaciones y documentos repartidos durante tu jornada.',
    },
    comparisons: {
      key: 'Comparisons',
      title: 'Comparativas de herramientas de memoria de IA',
      description: 'Comparaciones claras entre ShogunAI y herramientas de notas, búsqueda, memoria y conocimiento de equipo.',
    },
    privacy: {
      key: 'Privacy',
      title: 'IA privada y local-first',
      description: 'Cómo la memoria local-first cambia la captura, el almacenamiento y el uso del contexto de trabajo personal.',
    },
    product: {
      key: 'Product',
      title: 'Notas de producto de ShogunAI',
      description: 'Ideas de producto, lanzamientos y notas de campo al construir una capa privada de memoria de IA.',
    },
  },
  de: {
    'ai-memory': {
      key: 'AI Memory',
      title: 'KI-Gedächtnis für Wissensarbeit',
      description: 'Leitfäden zu KI-Gedächtnis, Kontext, Abruf und Systemen, mit denen du deine Arbeit nahtlos fortsetzt.',
    },
    'work-context': {
      key: 'Work Context',
      title: 'Arbeitskontext über deine Tools hinweg',
      description: 'Praktische Ideen zum Erfassen und Durchsuchen von Entscheidungen, Gesprächen und Dokumenten deines Arbeitstags.',
    },
    comparisons: {
      key: 'Comparisons',
      title: 'Vergleich von KI-Gedächtnis-Tools',
      description: 'Klare Vergleiche zwischen ShogunAI und Tools für Notizen, Suche, Gedächtnis und Teamwissen.',
    },
    privacy: {
      key: 'Privacy',
      title: 'Private und local-first KI',
      description: 'Wie local-first Gedächtnis die Erfassung, Speicherung und Nutzung persönlichen Arbeitskontexts verändert.',
    },
    product: {
      key: 'Product',
      title: 'ShogunAI Produktnotizen',
      description: 'Produktgedanken, Releases und Werkstattnotizen aus dem Aufbau einer privaten KI-Gedächtnisebene.',
    },
  },
};

export const BLOG_TOPIC_LABEL: Record<Locale, string> = {
  en: 'Topic',
  ja: 'トピック',
  es: 'Tema',
  de: 'Thema',
};

export function isBlogCategorySlug(value: string): value is BlogCategorySlug {
  return (BLOG_CATEGORY_SLUGS as readonly string[]).includes(value);
}

export function getBlogCategoryCopy(slug: BlogCategorySlug, locale: Locale) {
  return CATEGORY_COPY[locale][slug];
}
