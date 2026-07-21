import type { Metadata } from 'next';
import { Footer } from '@/components/sections/Footer';
import { Nav } from '@/components/sections/Nav';
import { StatusDashboard } from '@/components/StatusDashboard';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

// Private page — keep it out of search indexes.
export const metadata: Metadata = { robots: { index: false, follow: false } };
export const dynamic = 'force-dynamic';

export default async function StatusPage({
  searchParams,
}: {
  searchParams: Promise<{ code?: string; demo?: string }>;
}) {
  const { code, demo } = await searchParams;
  // ?demo=1 renders the page with sample data — no DB / private link needed,
  // so the design is viewable in any environment while the backend is wired up.
  const isDemo = demo === '1' || demo === 'true';

  return (
    <>
      <Nav />
      <main id="top" className="py-[clamp(56px,9vw,112px)]">
        <div className="container-x">
          {isDemo ? (
            <StatusDashboard code="demo" demo />
          ) : code ? (
            <StatusDashboard code={code} />
          ) : (
            <Card className="mx-auto grid max-w-md gap-4 text-center">
              <h1 className="font-display text-2xl font-semibold">No status code</h1>
              <p className="text-muted">Open your status page from the link in your welcome email.</p>
              <Button asChild className="justify-self-center">
                <a href="/">Back to home</a>
              </Button>
            </Card>
          )}
        </div>
      </main>
      <Footer />
    </>
  );
}
