import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import { LogoDefs } from '@/components/Logo';
import { JsonLd, organizationSchema, softwareApplicationSchema } from '@/components/seo/JsonLd';
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
    <html lang={locale} className={`${GeistSans.variable} ${GeistMono.variable} ${inter.variable}`}>
      <body className="min-h-dvh bg-surface font-sans text-ink antialiased">
        <JsonLd data={organizationSchema} />
        <JsonLd data={softwareApplicationSchema} />
        <LogoDefs />
        {children}
      </body>
    </html>
  );
}
