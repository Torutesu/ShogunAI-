import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import Home from '@/app/page';
import { isLocale, locales, type Locale } from '@/i18n/config';
import { siteConfig } from '@/lib/site';

export function generateStaticParams() {
  return locales.map((locale) => ({ locale }));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> {
  const { locale } = await params;
  if (!isLocale(locale)) return {};
  const copy = {
    en: {
      title: 'ShogunAI — Private AI memory assistant for work',
      description: 'A private, local-first AI memory assistant for macOS that recalls work context and turns decisions into action across your tools.',
      og: 'en_US',
    },
    ja: {
      title: 'ShogunAI — 仕事を記憶して動くプライベートAI',
      description: 'macOS向けのローカルファーストなAIメモリ。仕事の文脈を記憶・検索し、判断から次のアクションまでつなげます。',
      og: 'ja_JP',
    },
    es: {
      title: 'ShogunAI — Memoria de IA privada para tu trabajo',
      description: 'Memoria de IA privada y local-first para macOS que recupera el contexto de trabajo y convierte tus decisiones en acciones.',
      og: 'es_ES',
    },
    de: {
      title: 'ShogunAI — Privates KI-Gedächtnis für deine Arbeit',
      description: 'Ein privates, local-first KI-Gedächtnis für macOS, das Arbeitskontext abruft und Entscheidungen über deine Tools in Handlungen verwandelt.',
      og: 'de_DE',
    },
  }[locale];
  return {
    title: { absolute: copy.title },
    description: copy.description,
    alternates: {
      canonical: `/${locale}`,
      languages: {
        en: `${siteConfig.url}/en`,
        ja: `${siteConfig.url}/ja`,
        es: `${siteConfig.url}/es`,
        de: `${siteConfig.url}/de`,
        'x-default': `${siteConfig.url}/en`,
      },
    },
    openGraph: { locale: copy.og, url: `${siteConfig.url}/${locale}`, title: copy.title, description: copy.description },
    twitter: { card: 'summary_large_image', title: copy.title, description: copy.description, images: ['/og-image.png'] },
  };
}

export default async function LocalizedHome({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return <Home localeOverride={locale as Locale} />;
}
