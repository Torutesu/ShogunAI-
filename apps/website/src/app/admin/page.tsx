import type { Metadata } from 'next';
import { AdminDashboard } from '@/components/AdminDashboard';

export const dynamic = 'force-dynamic';
export const metadata: Metadata = { title: 'Admin', robots: { index: false, follow: false } };

export default function AdminPage() {
  return (
    <main className="container-x py-14">
      <AdminDashboard />
    </main>
  );
}
