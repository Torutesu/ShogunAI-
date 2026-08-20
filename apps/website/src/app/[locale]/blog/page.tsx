import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import BlogIndex from '@/app/blog/page';
import { isLocale, locales } from '@/i18n/config';
import { siteConfig } from '@/lib/site';

export function generateStaticParams() {
  return locales.map((locale) => ({ locale }));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> {
  const { locale } = await params;
  if (!isLocale(locale)) return {};
  const copy = {
    en: {
      title: 'ShogunAI Blog — AI memory, work context, and privacy',
      description: 'Practical guides to private AI memory, work context, local-first privacy, and connecting the tools knowledge workers use every day.',
    },
    ja: {
      title: 'ShogunAIブログ — AIメモリ、仕事の文脈、プライバシー',
      description: 'AIメモリ、仕事の文脈、ローカルファーストなプライバシー、日々のツールをつなぐ実践ガイド。',
    },
    es: {
      title: 'Blog de ShogunAI — Memoria de IA, contexto de trabajo y privacidad',
      description: 'Guías prácticas sobre memoria privada de IA, contexto de trabajo, privacidad local-first y las herramientas diarias de los profesionales del conocimiento.',
    },
    de: {
      title: 'ShogunAI Blog — KI-Gedächtnis, Arbeitskontext und Datenschutz',
      description: 'Praxisnahe Leitfäden zu privatem KI-Gedächtnis, Arbeitskontext, local-first Datenschutz und den täglichen Tools von Wissensarbeitern.',
    },
  }[locale];
  return {
    title: copy.title,
    description: copy.description,
    alternates: {
      canonical: `/${locale}/blog`,
      languages: { en: `${siteConfig.url}/en/blog`, ja: `${siteConfig.url}/ja/blog`, es: `${siteConfig.url}/es/blog`, de: `${siteConfig.url}/de/blog`, 'x-default': `${siteConfig.url}/en/blog` },
    },
  };
}

export default async function LocalizedBlog({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return <BlogIndex searchParams={Promise.resolve({ _locale: locale })} />;
}
