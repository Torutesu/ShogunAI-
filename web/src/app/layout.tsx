import type { Metadata } from 'next';
import { LogoDefs } from '@/components/Logo';
import './tokens.css';
import './page-lp.css';
import './globals.css';

export const metadata: Metadata = {
  title: 'ShogunAI — The AI that remembers your day and acts on it',
  description:
    'ShogunAI quietly captures your work, builds a private memory, and turns it into action. Memory that captures your day. Execution that acts on it.',
  openGraph: {
    title: 'ShogunAI — Memory that acts',
    description: 'An AI with memory that captures your day and execution that acts on it.',
    type: 'website',
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <LogoDefs />
        {children}
      </body>
    </html>
  );
}
