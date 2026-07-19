import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import { LogoDefs } from '@/components/Logo';
import { ThemeScript } from '@/components/ThemeToggle';
import { JsonLd, organizationSchema, softwareApplicationSchema, websiteSchema } from '@/components/seo/JsonLd';
import { getLocale } from '@/i18n/server';
import { siteConfig } from '@/lib/site';
import './globals.css';

const inter = Inter({ subsets: ['latin'], variable: '--font-inter', display: 'swap' });

export const metadata: Metadata = {
  metadataBase: new URL(siteConfig.url),
  title: {
    default: `${siteConfig.name} — ${siteConfig.tagline}`,
    template: `%s · ${siteConfig.name}`,
  },
  description: siteConfig.description,
  applicationName: siteConfig.name,
  alternates: { canonical: '/' },
  openGraph: {
    type: 'website',
    siteName: siteConfig.name,
    title: `${siteConfig.name} — Memory that acts`,
    description: siteConfig.description,
    url: siteConfig.url,
  },
  twitter: {
    card: 'summary_large_image',
    site: siteConfig.twitter,
    title: `${siteConfig.name} — Memory that acts`,
    description: siteConfig.description,
  },
  robots: { index: true, follow: true },
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={`${GeistSans.variable} ${GeistMono.variable} ${inter.variable}`}
    >
      <head>
        <ThemeScript />
      </head>
      <body className="min-h-dvh bg-bg font-sans text-ink antialiased">
        <JsonLd data={organizationSchema} />
        <JsonLd data={softwareApplicationSchema} />
        <JsonLd data={websiteSchema} />
        <LogoDefs />
        {children}
      </body>
    </html>
  );
}
