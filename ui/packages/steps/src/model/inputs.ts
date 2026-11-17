/**
 * @file What the step editor's node is connected to, as the host reports it:
 * the table the steps start from, and every other input by the name a step
 * uses for it. An input is a table, a single value or a list, each with its
 * dtypes. The view keeps none of it; it asks the host.
 */

import type { InputView, NodeView } from '@dageist/host';

import type { Frame } from './frame';

/** A table a step can join to, by the name the step refers to it with. */
export interface NamedTable {
  readonly name: string;
  readonly columns: Frame;
}

/** Returns the input called `name`, if the node has one. */
export function findInput(node: NodeView, name: string): InputView | null {
  return node.inputs.find((input) => input.name === name) ?? null;
}

/** Returns the connected tables a step can join to or pivot by. */
export function tablesOf(node: NodeView): readonly NamedTable[] {
  return node.inputs.flatMap((input) =>
    input.kind === 'table' ? [{ name: input.name, columns: input.columns }] : [],
  );
}

/** Returns the table called `name`, if there is one. */
export function findTable(node: NodeView, name: string): NamedTable | null {
  return tablesOf(node).find((table) => table.name === name) ?? null;
}
