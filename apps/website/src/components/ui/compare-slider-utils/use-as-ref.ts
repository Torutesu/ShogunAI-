import * as React from 'react';

/** Keep the latest value in a ref, so callbacks read it without re-subscribing. */
export function useAsRef<T>(value: T) {
  const ref = React.useRef(value);
  React.useEffect(() => {
    ref.current = value;
  });
  return ref;
}
