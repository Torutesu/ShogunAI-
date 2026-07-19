import { Nav } from '@/components/Nav';
import { Footer } from '@/components/Footer';
import { StatusDashboard } from '@/components/StatusDashboard';
import type { Metadata } from 'next';

// Private page — keep it out of search indexes.
export const metadata: Metadata = { robots: { index: false, follow: false } };
export const dynamic = 'force-dynamic';

export default async function StatusPage({
  searchParams,
}: {
  searchParams: Promise<{ code?: string }>;
}) {
  const { code } = await searchParams;

  return (
    <>
      <Nav />
      <main id="top">
        <section className="section">
          <div className="container">
            {code ? (
              <StatusDashboard code={code} />
            ) : (
              <div className="dash card center stack">
                <h1 className="t-h2">No status code</h1>
                <p className="muted">Open your status page from the link in your welcome email.</p>
                <a className="btn btn-primary" href="/" style={{ justifySelf: 'center' }}>Back to home</a>
              </div>
            )}
          </div>
        </section>
      </main>
      <Footer />
    </>
  );
}
