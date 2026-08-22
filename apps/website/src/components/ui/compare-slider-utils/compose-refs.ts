import * as React from 'react';

type PossibleRef<T> = React.Ref<T> | undefined;

function setRef<T>(ref: PossibleRef<T>, value: T) {
  if (typeof ref === 'function') return ref(value);
  if (ref !== null && ref !== undefined) (ref as React.RefObject<T>).current = value;
  return undefined;
}

/** Point several refs at the same node. */
function composeRefs<T>(...refs: PossibleRef<T>[]) {
  return (node: T) => {
    const cleanups = refs.map((ref) => setRef(ref, node));
    return () => {
      for (const [index, cleanup] of cleanups.entries()) {
        if (typeof cleanup === 'function') cleanup();
        else setRef(refs[index], null as T);
      }
    };
  };
}

function useComposedRefs<T>(...refs: PossibleRef<T>[]) {
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return React.useCallback(composeRefs(...refs), refs);
}

export { composeRefs, useComposedRefs };
