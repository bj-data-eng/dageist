/**
 * @file How the step editor writes a cell as text.
 */

import type { Cell, Dtype } from './model/frame';

/** Writes a duration held as whole minutes the way Polars prints one, such as `1d 7h 25m`. */
function formatDuration(minutes: number): string {
  const size = Math.abs(minutes);
  const parts = [
    [Math.floor(size / 1440), 'd'],
    [Math.floor((size % 1440) / 60), 'h'],
    [size % 60, 'm'],
  ] as const;
  const text = parts
    .filter(([amount]) => amount > 0)
    .map(([amount, unit]) => `${String(amount)}${unit}`)
    .join(' ');
  if (text === '') return '0m';
  return minutes < 0 ? `-${text}` : text;
}

/** Returns the text shown for `cell` of a column of `dtype`. */
export function formatCell(cell: Cell, dtype: Dtype): string {
  if (cell === null) return '—';
  if (dtype === 'Duration' && typeof cell === 'number') return formatDuration(cell);
  return String(cell);
}
