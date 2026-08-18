'use client';

import { Moon, Sun } from 'lucide-react';
import { useEffect, useState } from 'react';

type Theme = 'light' | 'dark';

/** Light/dark toggle. Persists to localStorage; syncs the root data-theme. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme | null>(null);

  useEffect(() => {
    const root = document.documentElement;
    const stored = root.getAttribute('data-theme') as Theme | null;
    // The landing page defaults to its light sky palette. Dark mode remains an
    // explicit visitor choice, rather than inheriting an OS setting mid-layout.
    const initial = stored ?? 'light';
    setTheme(initial);
  }, []);

  function toggle() {
    const next: Theme = theme === 'dark' ? 'light' : 'dark';
    setTheme(next);
    document.documentElement.setAttribute('data-theme', next);
    try {
      localStorage.setItem('theme', next);
    } catch {
      /* ignore */
    }
  }

  return (
    <button
      type="button"
      onClick={toggle}
      aria-label="Toggle color theme"
      className="group flex size-11 items-center justify-center rounded-full border border-border text-muted transition-colors hover:border-ink/25 hover:text-ink"
    >
      {theme === 'dark' ? (
        <Sun className="size-4 transition-transform duration-500 group-hover:rotate-90" />
      ) : (
        <Moon className="size-4 transition-transform duration-500 group-hover:-rotate-12" />
      )}
    </button>
  );
}

/** Inline, render-blocking script that sets the theme before first paint. */
export function ThemeScript() {
  const js = `(function(){try{var t=localStorage.getItem('theme');if(t==='dark'||t==='light'){document.documentElement.setAttribute('data-theme',t);}}catch(e){}})();`;
  // eslint-disable-next-line react/no-danger
  return <script dangerouslySetInnerHTML={{ __html: js }} />;
}
