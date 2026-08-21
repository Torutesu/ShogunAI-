import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import type { Metadata } from 'next';
import { CustomCursor } from '@/components/CustomCursor';
import { ThemeScript } from '@/components/ThemeToggle';
import { JsonLd, organizationSchema, softwareApplicationSchema, websiteSchema } from '@/components/seo/JsonLd';
import { getLocale } from '@/i18n/server';
import { siteConfig } from '@/lib/site';
import './globals.css';

export const metadata: Metadata = {
  metadataBase: new URL(siteConfig.url),
  title: {
    default: `${siteConfig.name} — ${siteConfig.tagline}`,
    template: `%s · ${siteConfig.name}`,
  },
  description: siteConfig.description,
  keywords: [
    'AI memory assistant',
    'private AI memory',
    'local-first AI',
    'AI memory for work',
    'knowledge worker productivity',
    'macOS AI assistant',
    'BYOK AI',
  ],
  applicationName: siteConfig.name,
  alternates: {
    canonical: '/',
    languages: {
      en: '/en',
      ja: '/ja',
      es: '/es',
      de: '/de',
      'x-default': '/',
    },
  },
  icons: {
    icon: [
      // Use the canonical current mark first so browsers do not prefer the legacy PNG.
      { url: '/product-icon.svg?v=20260815', type: 'image/svg+xml', sizes: 'any' },
    ],
    apple: [{ url: '/apple-touch-icon.png?v=20260815', type: 'image/png', sizes: '512x512' }],
  },
  openGraph: {
    type: 'website',
    siteName: siteConfig.name,
    title: `${siteConfig.name} — ${siteConfig.tagline}`,
    description: siteConfig.description,
    url: siteConfig.url,
    images: [{ url: '/og-image.png?v=20260821', width: 1200, height: 630, alt: 'ShogunAI — Your personal AGI on your PC.' }],
  },
  twitter: {
    card: 'summary_large_image',
    site: siteConfig.twitter,
    title: `${siteConfig.name} — ${siteConfig.tagline}`,
    description: siteConfig.description,
    images: ['/og-image.png?v=20260821'],
  },
  robots: { index: true, follow: true },
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={`${GeistSans.variable} ${GeistMono.variable}`}
    >
      <head>
        <ThemeScript />
      </head>
      <body className="min-h-dvh bg-bg font-sans text-ink antialiased">
        <JsonLd data={organizationSchema} />
        <JsonLd data={softwareApplicationSchema} />
        <JsonLd data={websiteSchema} />
        <CustomCursor />
        {children}
      </body>
    </html>
  );
}
