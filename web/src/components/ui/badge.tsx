import * as React from 'react';
import { cn } from '@/lib/utils';

/** Sky-tinted pill used for eyebrows and status chips. */
export function Badge({
  className,
  children,
  dot = false,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { dot?: boolean }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-2 rounded-full bg-sky-soft px-3 py-1.5 text-xs font-medium tracking-[0.02em] text-ink',
        className,
      )}
      {...props}
    >
      {dot && <span className="size-1.5 rounded-full bg-accent" aria-hidden="true" />}
      {children}
    </span>
  );
}
