import * as React from 'react';
import { cn } from '@/lib/utils';

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        'h-12 w-full rounded-full border border-border/80 bg-white/88 px-5 text-[15px] text-ink shadow-[inset_0_1px_0_rgba(255,255,255,0.55)]',
        'placeholder:text-faint focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';
