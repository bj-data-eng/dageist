/**
 * @file The other tables a step can look values up in: the node's connected
 * tables, apart from the one its steps start from.
 */

import type { Frame } from './frame';
import { INPUTS } from './inputs';

/** A table a step can join to, by the name the step refers to it with. */
export interface NamedTable {
  readonly name: string;
  readonly frame: Frame;
}

/** The connected tables a step can join to. */
export const TABLES: readonly NamedTable[] = INPUTS.flatMap((input) =>
  input.kind === 'table' && !input.primary ? [{ name: input.name, frame: input.frame }] : [],
);

/** Returns the table called `name`, if there is one. */
export function findTable(name: string): NamedTable | null {
  return TABLES.find((table) => table.name === name) ?? null;
}
