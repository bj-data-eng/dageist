/**
 * @file The word: a button written as nothing but its letters, in the colour
 * of what it names. Everything that can be changed in a line of "code" is
 * one, so a column, a method, a value and a comparison are told apart by
 * colour, as in a code editor, and not by a box. Nothing about a word
 * changes when it is pointed at. The quiet things that stand between words
 * are here too: a word that names a parameter, a caret and an equals sign.
 */

import { cx } from './cx';

/** What a block names, which decides its colour. */
export type Tone =
  | 'column'
  | 'function'
  | 'value'
  | 'compare'
  | 'input'
  | 'table'
  | 'ink'
  | 'verb'
  | 'add'
  | 'remove';

/**
 * How a block is written: plainly, as a verb, as a comparison, as text
 * between drawn quotation marks, as nothing yet, or as three dots.
 */
export type Look = 'plain' | 'verb' | 'compare' | 'quoted' | 'none' | 'dots';

/** What a block shows and reports. */
export interface BlockProps {
  readonly label: string;
  readonly tone: Tone;
  readonly look: Look;
  /** What the block is, for assistive technology, when its label alone does not say. */
  readonly describedAs?: string;
  /** Whether what the block holds cannot be worked out. */
  readonly wrong?: boolean;
  /** Whether the block is a faint word that waits where the next thing goes. */
  readonly waits?: boolean;
  /** Whether the list the block opens is open, for a block that opens one. */
  readonly expanded?: boolean;
  /** Receives the button, for a caller that places something against it or drags by it. */
  readonly buttonRef?: (element: HTMLButtonElement | null) => void;
  readonly onPress: (button: HTMLButtonElement) => void;
}

/** Shows `label` as a word that can be pressed. */
export function Block({
  label,
  tone,
  look,
  describedAs,
  wrong = false,
  waits = false,
  expanded,
  buttonRef,
  onPress,
}: BlockProps): React.JSX.Element {
  return (
    <button
      ref={buttonRef}
      type="button"
      data-tone={tone}
      data-look={look}
      aria-label={describedAs}
      aria-expanded={expanded}
      className={cx('dg-block', wrong && 'dg-wrong', waits && 'dg-waits')}
      onClick={(event) => {
        onPress(event.currentTarget);
      }}
    >
      {label}
    </button>
  );
}

/**
 * A quiet word that names the parameter after it, as `by` does after Sort
 * table. It is not pressed.
 */
export function Word({ children }: { readonly children: string }): React.JSX.Element {
  return <i className="dg-quiet">{children}</i>;
}

/** The caret between one part of a chain and the next. */
export function Seam(): React.JSX.Element {
  return (
    <i aria-hidden="true" className="dg-quiet">
      ›
    </i>
  );
}

/** The equals sign between the name a step makes and what it holds. */
export function Equals(): React.JSX.Element {
  return <i className="dg-quiet">=</i>;
}
