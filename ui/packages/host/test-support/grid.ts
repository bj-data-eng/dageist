/**
 * @file Test support: reads `offers-grid.json`, which the host's tests write
 * from the offers command: for every context and every kind of what an
 * expression may have, the entries that may stand there. It is not something
 * the host sends, so it is not part of the package's front door.
 */

import type { AfterView, ContextView } from '../src/contract';
import type { Decode } from '../src/decode';
import { ROOT, readCount, readField, readList, readRecord, readString } from '../src/decode';
import { readAfter, readContext } from '../src/decode-run';

/** What the host wrote: each place, and which entries may stand there. */
export interface Grid {
  /** The entries that stand in an expression, in the order of the bits. */
  readonly entries: readonly string[];
  readonly contexts: readonly ContextView[];
  readonly afters: readonly AfterView[];
  /**
   * For each context, for each of `afters`, words of 32 bits with a bit set for each entry that may
   * stand: the bit `entry % 32` of the word `entry / 32`.
   */
  readonly allowed: ReadonlyArray<ReadonlyArray<readonly number[]>>;
}

const readGrid: Decode<Grid> = readRecord(
  ['entries', 'contexts', 'afters', 'allowed'],
  (fields, path) => ({
    entries: readField(fields, path, 'entries', readList(readString)),
    contexts: readField(fields, path, 'contexts', readList(readContext)),
    afters: readField(fields, path, 'afters', readList(readAfter)),
    allowed: readField(fields, path, 'allowed', readList(readList(readList(readCount)))),
  }),
);

/** Reads the grid the host wrote, refusing what is not one. */
export function decodeGrid(value: unknown): Grid {
  return readGrid(value, ROOT);
}

/**
 * Returns the entries that may stand at the place of the context `context` and the kind `after` of the grid.
 *
 * @throws {Error} When the place is not in the grid, when its words are not as many as the entries need, or when a word is not a number of 32 bits.
 */
export function allowedAt(grid: Grid, context: number, after: number): readonly string[] {
  const words = grid.allowed[context]?.[after];
  if (words === undefined) throw new Error(`the grid has no place ${context}, ${after}`);
  const needed = Math.ceil(grid.entries.length / 32);
  if (words.length !== needed) {
    throw new Error(`the place ${context}, ${after} has ${words.length} words, not ${needed}`);
  }
  for (const word of words) {
    if (!Number.isInteger(word) || word < 0 || word >= 2 ** 32) {
      throw new Error(`the place ${context}, ${after} has a word of ${word}, not of 32 bits`);
    }
  }
  return grid.entries.filter((_entry, bit) => {
    const word = words[Math.floor(bit / 32)] ?? 0;
    return Math.floor(word / 2 ** (bit % 32)) % 2 === 1;
  });
}
