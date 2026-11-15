/**
 * @file Lines under a step. Whatever a step takes that is more than one thing
 * is written on lines of its own beneath it, each set in under what it
 * belongs to, as code is. A line can hold lines of its own, set in again.
 */

import { createContext } from 'react';

/** Something just added, which is to be gone to as soon as it is shown. */
export interface Fresh {
  /** The id of what was just added, or `null`. */
  readonly id: string | null;
  /** Says that the thing with `id` was just added. */
  readonly mark: (id: string) => void;
}

/** What was just added, so that its first slot opens or its name is typed straight away. */
export const FreshContext = createContext<Fresh>({ id: null, mark: () => undefined });

/** Holds lines that belong to what stands before them. */
export function Lines({ children }: { readonly children: React.ReactNode }): React.JSX.Element {
  return <span className="dg-lines">{children}</span>;
}

/** One line: what it holds, and any lines of its own beneath it. */
export function Line({ children }: { readonly children: React.ReactNode }): React.JSX.Element {
  return <span className="dg-line">{children}</span>;
}
