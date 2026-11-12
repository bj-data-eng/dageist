/**
 * @file Menu choices the step editor's pickers share.
 */

import type { Choice } from '@dageist/design';

import type { Frame } from './model/frame';

/** Lists the columns of `frame`, each with its exact dtype. */
export function columnChoices(
  frame: Frame,
  current: string | null,
  onPick: (column: string) => void,
): Choice[] {
  return frame.map((column) => ({
    label: `${column.name} · ${column.dtype}`,
    current: column.name === current,
    reason: null,
    onPick: () => {
      onPick(column.name);
    },
  }));
}
