'use client';

import { Menu, X } from 'lucide-react';
import { AnimatePresence, motion, useReducedMotion } from 'motion/react';
import { useEffect, useRef, useState } from 'react';

type Item = { href: string; label: string };

/**
 * Mobile navigation. Unlike the old <details> disclosure this closes on
 * link tap, Escape and outside click, and animates open/close.
 */
export function MobileMenu({ items, cta }: { items: Item[]; cta: Item }) {
  const [open, setOpen] = useState(false);
  const reduce = useReducedMotion();
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('pointerdown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('pointerdown', onDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative lg:hidden">
      <button
        type="button"
        aria-label={open ? 'Close menu' : 'Open menu'}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex size-9 items-center justify-center rounded-full border border-border text-ink transition-colors hover:bg-cloud"
      >
        {open ? <X className="size-5" /> : <Menu className="size-5" />}
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={reduce ? false : { opacity: 0, y: -6, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={reduce ? { opacity: 0 } : { opacity: 0, y: -6, scale: 0.98 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="absolute right-0 top-11 w-56 origin-top-right rounded-xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]"
          >
            {items.map((l) => (
              <a
                key={l.href}
                href={l.href}
                onClick={() => setOpen(false)}
                className="block rounded-lg px-3 py-2 text-sm font-medium text-ink hover:bg-cloud"
              >
                {l.label}
              </a>
            ))}
            <a
              href={cta.href}
              onClick={() => setOpen(false)}
              className="mt-1 block rounded-lg bg-accent px-3 py-2.5 text-center text-sm font-semibold text-white shadow-[0_2px_10px_rgba(0,166,244,0.28)] transition-opacity hover:opacity-90"
            >
              {cta.label}
            </a>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
