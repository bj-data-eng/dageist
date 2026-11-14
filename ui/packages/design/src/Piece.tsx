/**
 * @file The piece: a body of blocks, the square brackets round it when it is
 * a function or a step, and a panel at each side. Pointing at a piece parts
 * its brackets and uncovers the panels: a cross on the left takes the piece
 * away and a plus on the right adds something after it. A click is for what
 * is inside the piece; pointing is for the piece itself.
 */

import { useState } from 'react';

import type { Tone } from './Block';
import { cx } from './cx';

/** What one side panel of a piece is called and does. */
export interface SideAction {
  /** What the panel does, for assistive technology and as its title. */
  readonly label: string;
  /** Called with the piece, so that a list can be placed against it. */
  readonly onPress: (piece: HTMLElement) => void;
}

/** What a piece shows and reports. */
export interface PieceProps {
  /** The colour of the piece's brackets, and of the blocks in it that name no colour. */
  readonly tone: Tone;
  /** Whether the piece stands in square brackets, as a function and a step do. */
  readonly bracketed: boolean;
  /** Takes the piece away, or `null` when it cannot be taken away. */
  readonly remove: SideAction | null;
  /** Adds something after the piece, or `null` when nothing can follow it. */
  readonly add: SideAction | null;
  /** Whether the piece is being dragged to another place. */
  readonly dragged?: boolean;
  /** Receives the piece, for a caller that drags it. */
  readonly pieceRef?: (element: HTMLElement | null) => void;
  readonly children: React.ReactNode;
}

/** Draws `children` as one piece with its side panels. */
export function Piece({
  tone,
  bracketed,
  remove,
  add,
  dragged = false,
  pieceRef,
  children,
}: PieceProps): React.JSX.Element {
  const [piece, setPiece] = useState<HTMLElement | null>(null);
  return (
    <span
      ref={(element) => {
        setPiece(element);
        pieceRef?.(element);
      }}
      data-tone={tone}
      className={cx('dg-piece', bracketed && 'dg-bracketed', dragged && 'dg-dragged')}
    >
      {remove !== null && (
        <button
          type="button"
          aria-label={remove.label}
          title={remove.label}
          className="dg-side dg-side-remove"
          onClick={() => {
            if (piece !== null) remove.onPress(piece);
          }}
        >
          ×
        </button>
      )}
      <span className="dg-body">{children}</span>
      {add !== null && (
        <button
          type="button"
          aria-label={add.label}
          title={add.label}
          className="dg-side dg-side-add"
          onClick={() => {
            if (piece !== null) add.onPress(piece);
          }}
        >
          +
        </button>
      )}
    </span>
  );
}
