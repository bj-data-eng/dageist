/**
 * @file Lays out the first values of a column or a list for a line of text.
 * The values are written by the engine; only their layout is the view's.
 */

import type { ArgumentView, ValuesView } from '@dageist/host';

/** What stands for a null, which the engine writes no text for. */
export const NULL_DASH = '—';

/** Returns the values as one line: each as the engine wrote it, a null as a dash, and an ellipsis where more follow. */
export function valuesLine(values: ValuesView): string {
  const shown = values.shown.map((value) => cellText(value)).join(', ');
  return values.more > 0 ? `${shown}, …` : shown;
}

/** Returns the text of a cell of the rows: as the engine wrote it, or a dash for a null, which it writes no text for. */
export function cellText(cell: string | null): string {
  return cell ?? NULL_DASH;
}

/** How a number, text, duration, pattern, path or time zone typed in is shown by what the document holds there. */
export interface Shown {
  /** Whether it is drawn as text, between quotation marks. */
  readonly text: boolean;
  /** The words the engine shows it as. */
  readonly shown: string;
}

/**
 * Returns how an argument that holds something typed in is shown, or `null` where it holds nothing
 * the editor shows: no argument, a time zone that is none, and a kind of argument that is not typed
 * in. A flag is one of them, so a flag typed in as a value is not shown (#177).
 */
export function shownOf(argument: ArgumentView | undefined): Shown | null {
  switch (argument?.kind) {
    case 'integer':
    case 'float':
      return { text: false, shown: argument.shown };
    case 'string':
      return { text: true, shown: argument.value };
    case 'duration':
    case 'pattern':
    case 'path':
      return { text: true, shown: argument.text };
    case 'field':
      return { text: true, shown: argument.name };
    case 'timeZone':
      return argument.zone === null ? null : { text: true, shown: argument.zone };
    default:
      return null;
  }
}
