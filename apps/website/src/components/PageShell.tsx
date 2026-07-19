import { Footer } from '@/components/sections/Footer';
import { Nav } from '@/components/sections/Nav';

/** Nav + main + Footer wrapper for standalone pages. */
export function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <>
      <Nav />
      <main id="top" className="min-h-[60vh]">
        {children}
      </main>
      <Footer />
    </>
  );
}

export function PageHeader({
  eyebrow,
  title,
  sub,
}: {
  eyebrow: string;
  title: string;
  sub?: string;
}) {
  return (
    <header className="border-b border-border bg-[radial-gradient(120%_100%_at_50%_-40%,var(--color-sky-soft)_0%,transparent_60%)]">
      <div className="container-x py-[clamp(30px,7.5vw,88px)] text-center">
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{eyebrow}</p>
        <h1 className="mx-auto mt-3.5 max-w-[24ch] font-display text-[clamp(30px,4.5vw,48px)] font-semibold leading-[1.08] tracking-[-0.02em] text-balance">
          {title}
        </h1>
        {sub && <p className="mx-auto mt-4 max-w-[52ch] text-[17px] leading-relaxed text-muted">{sub}</p>}
      </div>
    </header>
  );
}
