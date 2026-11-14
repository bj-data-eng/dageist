/**
 * @file The block: a button in the colour of what it names. Everything that
 * can be pressed in a row of "code" is one, so a column, a method, a value
 * and a comparison are told apart by colour and not by shape. The small
 * things that stand between blocks are here too: a word, a caret and an
 * equals sign.
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

/** Shows `label` as a block that can be pressed. */
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

/** What a word shows. */
export interface WordProps {
  /** Whether the word follows a value, so that it starts a block of its own. */
  readonly apart: boolean;
  readonly children: string;
}

/**
 * A word inside a piece that names the parameter after it, as `by` does
 * after Sort table. It is not pressed: it carries on the block before it.
 */
export function Word({ apart, children }: WordProps): React.JSX.Element {
  return <span className={cx('dg-word', apart && 'dg-apart')}>{children}</span>;
}

/** The caret between one block of a chain and the next. A faint one leads a word that waits. */
export function Seam({ waits }: { readonly waits: boolean }): React.JSX.Element {
  return <span aria-hidden="true" className={cx('dg-seam', waits && 'dg-waits')} />;
}

/** The equals sign between the name a step makes and the chain that makes it. */
export function Equals(): React.JSX.Element {
  return (
    <span className="dg-equals">
      <span className="sr-only">is</span>
    </span>
  );
}
