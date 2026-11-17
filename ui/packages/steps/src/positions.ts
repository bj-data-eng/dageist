/**
 * @file Where the expression being drawn stands, as the positions that hold
 * it, so that a list asks the engine what may stand there without working out
 * a context. A step's parameter gives the first position; a parameter inside
 * an expression, such as a window's key, a condition's predicate or its
 * values, gives the next.
 */

import { createContext, useContext, useState } from 'react';

import type { PositionView } from '@dageist/host';

import { NO_POSITIONS } from './model/place';

/** The positions that hold the expression being drawn, outermost first. Empty where none is known. */
export const PositionsContext = createContext<readonly PositionView[]>(NO_POSITIONS);

/** Returns the positions that hold the expression being drawn. */
export function usePositions(): readonly PositionView[] {
  return useContext(PositionsContext);
}

/** What a list reports of itself, for the caller that asks the engine only while it is open. */
export interface ListState {
  /** Whether the list is open. */
  readonly open: boolean;
  /** What is typed into its search box; nothing while it is closed. */
  readonly typed: string;
  /** Pass to the slot or list whose state this is. */
  readonly onOpenChange: (open: boolean) => void;
  readonly onType: (typed: string) => void;
}

/** Returns the state of a list that the slot or list it is passed to reports to. */
export function useListState(): ListState {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState('');
  return {
    open,
    typed,
    onOpenChange: (now) => {
      setOpen(now);
      if (!now) setTyped('');
    },
    onType: setTyped,
  };
}
