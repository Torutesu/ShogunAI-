import type { Metadata } from 'next';
import { LegalPage } from '@/components/LegalPage';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Privacy Policy',
  description: 'How ShogunAI handles your data.',
  alternates: { canonical: '/privacy' },
};

export default async function PrivacyPage() {
  const { t } = await getI18n();
  return <LegalPage t={t} title={t.legalPage.privacyTitle} />;
}
