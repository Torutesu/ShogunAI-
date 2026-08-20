export const siteConfig = {
  name: 'ShogunAI',
  url: process.env.NEXT_PUBLIC_APP_ORIGIN ?? 'https://shogunaios.com',
  tagline: 'Your personal AGI on your PC. Built to finish real work.',
  description:
    'ShogunAI is a private, local-first AI memory assistant for macOS that recalls work context and turns decisions into action across your tools.',
  twitter: '@shogunai',
  locales: ['en', 'ja', 'es', 'de'] as const,
} as const;

export function localizedAlternates(path: string) {
  return {
    en: `${siteConfig.url}/en${path}`,
    ja: `${siteConfig.url}/ja${path}`,
    es: `${siteConfig.url}/es${path}`,
    de: `${siteConfig.url}/de${path}`,
    'x-default': `${siteConfig.url}/en${path}`,
  };
}
