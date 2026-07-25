import { Slot } from '@radix-ui/react-slot';
import { type VariantProps, cva } from 'class-variance-authority';
import * as React from 'react';
import { cn } from '@/lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full font-medium leading-none transition-all duration-200 ease-[var(--ease-out-soft)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:translate-y-px active:scale-[0.985] disabled:pointer-events-none disabled:opacity-60',
  {
    variants: {
      variant: {
        primary:
          'bg-[linear-gradient(135deg,var(--color-ink),color-mix(in_oklab,var(--color-ink)_82%,var(--color-accent)_18%))] text-on-ink shadow-[0_14px_30px_rgba(9,11,12,0.14)] hover:-translate-y-0.5 hover:shadow-[0_18px_34px_rgba(9,11,12,0.18)]',
        secondary: 'border border-border bg-white/70 text-ink hover:bg-cloud dark:bg-white/5',
        tertiary: 'rounded-none px-0 text-accent hover:text-accent-strong',
        ghost: 'bg-transparent text-ink hover:bg-cloud',
      },
      size: {
        sm: 'h-9 px-4 text-sm',
        md: 'h-12 px-6 text-[15px]',
        lg: 'h-12 px-7 text-base',
      },
    },
    defaultVariants: { variant: 'primary', size: 'md' },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button';
    return (
      <Comp ref={ref} className={cn(buttonVariants({ variant, size }), className)} {...props} />
    );
  },
);
Button.displayName = 'Button';

export { buttonVariants };
