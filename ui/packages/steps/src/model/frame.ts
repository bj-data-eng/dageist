/**
 * @file The columns a step is given, as the engine reports them: a name and
 * the engine's short name of its dtype, such as `Datetime(us)`. The view
 * holds no rows and no rules about dtypes of its own; both come from the
 * engine.
 */

import type { FieldView } from '@dageist/host';

/** The engine's short name of a dtype, such as `Int64` or `Datetime(us)`. */
export type Dtype = string;

/** One column of a table: its name and its dtype. */
export type Column = FieldView;

/** The columns of a table, in order. */
export type Frame = readonly Column[];

/** Fails on a value the caller's `switch` was supposed to have covered. */
export function assertNever(unexpected: never): never {
  throw new Error(`Unexpected value: ${String(unexpected)}`);
}

/** Returns the column of `frame` called `name`, if there is one. */
export function findColumn(frame: Frame, name: string): Column | null {
  return frame.find((column) => column.name === name) ?? null;
}
