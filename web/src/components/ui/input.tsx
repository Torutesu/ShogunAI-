import * as React from 'react';
import { cn } from '@/lib/utils';

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        'h-11 w-full rounded-full border border-border bg-surface px-[18px] text-[15px] text-ink',
        'placeholder:text-faint focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';
