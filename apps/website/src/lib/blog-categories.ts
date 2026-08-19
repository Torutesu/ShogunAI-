import type { Locale } from '@/i18n/config';

export const BLOG_CATEGORY_SLUGS = ['ideas', 'product'] as const;

export type BlogCategorySlug = (typeof BLOG_CATEGORY_SLUGS)[number];

type CategoryCopy = {
  key: string;
  title: string;
  description: string;
};

// `key` is the canonical value stored in each post's frontmatter and is the
// same in every locale; only the surrounding copy is translated. Display
// labels for the filter pills live in the dictionaries.
const CATEGORY_COPY: Record<Locale, Record<BlogCategorySlug, CategoryCopy>> = {
  en: {
    ideas: {
      key: 'Ideas',
      title: 'Ideas behind the product',
      description: 'Why memory belongs on your machine, what passive capture should and should not do, and where the memory layer is heading.',
    },
    product: {
      key: 'Product',
      title: 'ShogunAI product notes',
      description: 'How the product actually works: the execution layer, meeting memory, visual recall, and the rules around each of them.',
    },
  },
  ja: {
    ideas: {
      key: 'Ideas',
      title: 'プロダクトの背後にある思想',
      description: '記憶が自分の端末にあるべき理由、受動的なキャプチャがやるべきこととやらないこと、そしてメモリーレイヤーの行き先について。',
    },
    product: {
      key: 'Product',
      title: 'ShogunAI プロダクトノート',
      description: '実行レイヤー、会議の記憶、ビジュアルリコール ── 製品が実際にどう動き、それぞれにどんなルールを置いているか。',
    },
  },
  es: {
    ideas: {
      key: 'Ideas',
      title: 'Las ideas detrás del producto',
      description: 'Por qué la memoria debe vivir en tu equipo, qué debe y qué no debe hacer la captura pasiva, y hacia dónde va la capa de memoria.',
    },
    product: {
      key: 'Product',
      title: 'Notas de producto de ShogunAI',
      description: 'Cómo funciona el producto: la capa de ejecución, la memoria de reuniones, el recuerdo visual y las reglas de cada uno.',
    },
  },
  de: {
    ideas: {
      key: 'Ideas',
      title: 'Die Ideen hinter dem Produkt',
      description: 'Warum Gedächtnis auf dein Gerät gehört, was passive Erfassung tun und lassen sollte und wohin sich die Gedächtnisebene entwickelt.',
    },
    product: {
      key: 'Product',
      title: 'ShogunAI Produktnotizen',
      description: 'Wie das Produkt arbeitet: Ausführungsebene, Meeting-Gedächtnis, visuelle Erinnerung und die Regeln dahinter.',
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
