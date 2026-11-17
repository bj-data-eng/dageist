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
  /** For each context, for each of `afters`, a number with a bit set for each entry that may stand. */
  readonly allowed: ReadonlyArray<readonly number[]>;
}

const readGrid: Decode<Grid> = readRecord(
  ['entries', 'contexts', 'afters', 'allowed'],
  (fields, path) => ({
    entries: readField(fields, path, 'entries', readList(readString)),
    contexts: readField(fields, path, 'contexts', readList(readContext)),
    afters: readField(fields, path, 'afters', readList(readAfter)),
    allowed: readField(fields, path, 'allowed', readList(readList(readCount))),
  }),
);

/** Reads the grid the host wrote, refusing what is not one. */
export function decodeGrid(value: unknown): Grid {
  return readGrid(value, ROOT);
}

/** Returns the entries that may stand at the place of the context `context` and the kind `after` of the grid. */
export function allowedAt(grid: Grid, context: number, after: number): readonly string[] {
  const mask = grid.allowed[context]?.[after] ?? 0;
  return grid.entries.filter((_entry, bit) => Math.floor(mask / 2 ** bit) % 2 === 1);
}
