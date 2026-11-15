/**
 * @file The piece: the words that make up one thing, such as a method and
 * what it works with, or a step and what it takes. A piece draws nothing of
 * its own. It says what can be done to the thing as a whole, taking it away
 * and adding something after it, and the word that names the thing offers
 * both at the foot of its list.
 */

import { createContext, useMemo } from 'react';

import { cx } from './cx';

/** One thing that can be done to a piece as a whole. */
export interface SideAction {
  /** What it does, as the list says it. */
  readonly label: string;
  /** Called with the word that names the piece, so that a list can be placed against it. */
  readonly onPress: (anchor: HTMLElement) => void;
}

/** What can be done to the piece a word stands in. */
export interface PieceActions {
  /** Takes the piece away, or `null` when it cannot be taken away. */
  readonly remove: SideAction | null;
  /** Adds something after the piece, or `null` when nothing can follow it. */
  readonly add: SideAction | null;
}

/** What can be done to the nearest piece, for the word that names it. */
export const PieceContext = createContext<PieceActions>({ remove: null, add: null });

/** What a piece holds and what can be done to it. */
export interface PieceProps extends PieceActions {
  /** Whether the piece is being dragged to another place. */
  readonly dragged?: boolean;
  readonly children: React.ReactNode;
}

/** Holds `children` as one piece. */
export function Piece({ remove, add, dragged = false, children }: PieceProps): React.JSX.Element {
  const actions = useMemo(() => ({ remove, add }), [remove, add]);
  return (
    <PieceContext.Provider value={actions}>
      <span className={cx(dragged && 'dg-dragged')}>{children}</span>
    </PieceContext.Provider>
  );
}
