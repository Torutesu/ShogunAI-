'use client';

import { type HTMLMotionProps, motion, useReducedMotion } from 'motion/react';
import * as React from 'react';

type RevealProps = React.PropsWithChildren<{
  className?: string;
  delay?: number;
  y?: number;
}> &
  Omit<HTMLMotionProps<'div'>, 'ref'>;

/** Scroll-triggered fade + rise. Respects prefers-reduced-motion. */
export function Reveal({ children, className, delay = 0, y = 16, ...rest }: RevealProps) {
  const reduce = useReducedMotion();
  return (
    <motion.div
      className={className}
      initial={reduce ? false : { opacity: 0, y }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: '0px 0px -80px 0px' }}
      transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1], delay }}
      {...rest}
    >
      {children}
    </motion.div>
  );
}
