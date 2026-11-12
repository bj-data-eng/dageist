/**
 * @file How the step editor writes a cell as text.
 */

import type { Cell, Dtype } from './model/frame';

/** Writes a duration held as whole minutes the way Polars prints one, such as `1d 7h 25m`. */
function formatDuration(minutes: number): string {
  // Polars writes nothing as its smallest unit, and signs every part of a negative duration.
  if (minutes === 0) return '0µs';
  const parts = [
    [Math.trunc(minutes / 1440), 'd'],
    [Math.trunc((minutes % 1440) / 60), 'h'],
    [minutes % 60, 'm'],
  ] as const;
  return parts
    .filter(([amount]) => amount !== 0)
    .map(([amount, unit]) => `${String(amount)}${unit}`)
    .join(' ');
}

/** Returns the text shown for `cell` of a column of `dtype`. */
export function formatCell(cell: Cell, dtype: Dtype): string {
  if (cell === null) return '—';
  if (dtype === 'Duration' && typeof cell === 'number') return formatDuration(cell);
  return String(cell);
}
