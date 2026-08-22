import * as React from 'react';

/** useLayoutEffect on the client, useEffect on the server — no SSR warning. */
export const useIsomorphicLayoutEffect =
  typeof window !== 'undefined' ? React.useLayoutEffect : React.useEffect;
