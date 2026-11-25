/**
 * @file Lays out the first values of a column or a list for a line of text.
 * The values are written by the engine; only their layout is the view's.
 */

import type { ValuesView } from '@dageist/host';

/** What stands for a null, which the engine writes no text for. */
export const NULL_DASH = '—';

/** Returns the values as one line: each as the engine wrote it, a null as a dash, and an ellipsis where more follow. */
export function valuesLine(values: ValuesView): string {
  const shown = values.shown.map((value) => value ?? NULL_DASH).join(', ');
  return values.more > 0 ? `${shown}, …` : shown;
}

/** Returns the text of a cell of the rows: as the engine wrote it, or a dash for a null, which it writes no text for. */
export function cellText(cell: string | null): string {
  return cell ?? NULL_DASH;
}
