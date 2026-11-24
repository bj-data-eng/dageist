import * as React from 'react';

const MOBILE_BREAKPOINT = 768;

/** Calls `onChange` whenever the window crosses the phone-size breakpoint. */
function subscribe(onChange: () => void): () => void {
  const mql = window.matchMedia(`(max-width: ${String(MOBILE_BREAKPOINT - 1)}px)`);
  mql.addEventListener('change', onChange);
  return () => {
    mql.removeEventListener('change', onChange);
  };
}

/**
 * Reports whether the window is phone-sized. The original keeps the answer in
 * state set from an effect; this reads it from the window as it changes.
 */
export function useIsMobile(): boolean {
  return React.useSyncExternalStore(
    subscribe,
    () => window.innerWidth < MOBILE_BREAKPOINT,
    () => false,
  );
}
