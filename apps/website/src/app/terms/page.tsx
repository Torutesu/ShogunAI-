import type { Metadata } from 'next';
import { LegalPage } from '@/components/LegalPage';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Terms of Service',
  description: 'The terms for using ShogunAI.',
  alternates: { canonical: '/terms' },
};

export default async function TermsPage() {
  const { t } = await getI18n();
  return <LegalPage t={t} title={t.legalPage.termsTitle} />;
}
