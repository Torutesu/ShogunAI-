import type { Metadata } from 'next';
import { redirect } from 'next/navigation';

// Private page — keep it out of search indexes.
export const metadata: Metadata = { robots: { index: false, follow: false } };
export const dynamic = 'force-dynamic';

export default function StatusPage() {
  // Referral status pages are retired while early access is email-only.
  redirect('/');
}
