export const siteConfig = {
  name: 'ShogunAI',
  url: process.env.NEXT_PUBLIC_APP_ORIGIN ?? 'https://shogunai.com',
  tagline: 'The AI that remembers your day and acts on it',
  description:
    'ShogunAI quietly captures your work, builds a private memory, and turns it into action. Memory that captures your day. Execution that acts on it.',
  twitter: '@shogunai',
  locales: ['en', 'ja'] as const,
} as const;
