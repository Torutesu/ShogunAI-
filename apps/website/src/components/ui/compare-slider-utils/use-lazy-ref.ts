import * as React from 'react';

const UNSET = Symbol('unset');

/** A ref whose initial value is built once, on first render. */
export function useLazyRef<T>(init: () => T) {
  const ref = React.useRef<T | typeof UNSET>(UNSET);
  if (ref.current === UNSET) ref.current = init();
  return ref as React.RefObject<T>;
}
