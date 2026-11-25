/**
 * @file What the step editor's node is connected to, as the host reports it:
 * the table the steps start from, and every other input by the name a step
 * uses for it. An input is a table, a single value or a list, each with its
 * dtypes. The view keeps none of it; it asks the host.
 */

import type { InputView, NodeView, OriginView } from '@dageist/host';

import type { Frame } from './frame';
import { NULL_DASH, valuesLine } from './values';

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

/** Returns the words that say what supplies an input to the node. */
export function originWords(origin: OriginView): string {
  switch (origin.kind) {
    case 'node':
      return origin.name;
    case 'column':
      return `${origin.node} · column "${origin.column}"`;
    case 'parameter':
      return 'Parameters';
    default:
      return origin satisfies never;
  }
}

/** Returns the short description shown beside an input's name: how many columns a table has, a value's dtype, a list's element dtype. */
export function inputDetail(input: InputView): string {
  switch (input.kind) {
    case 'table':
      return `${String(input.columns.length)} columns`;
    case 'value':
      return input.dtype;
    case 'list':
      return `list of ${input.dtype}`;
    default:
      return input satisfies never;
  }
}

/** Returns the lines that describe an input: what it is, where it comes from and what it holds. */
export function inputLines(input: InputView): string[] {
  const from = `from ${originWords(input.from)}`;
  switch (input.kind) {
    case 'table':
      return [
        `${input.name} · table`,
        from,
        ...input.columns.map((column) => `${column.name} · ${column.dtype}`),
      ];
    case 'value':
      return [`${input.name} · ${input.dtype}`, from, input.value ?? NULL_DASH];
    case 'list':
      return [`${input.name} · list of ${input.dtype}`, from, valuesLine(input.values)];
    default:
      return input satisfies never;
  }
}

/** Returns the lines that describe the table the steps start from. */
export function sourceLines(node: NodeView): string[] {
  return [
    `${node.source.name} · table · the steps start from its rows`,
    `from ${originWords(node.source.from)}`,
    ...node.source.columns.map((column) => `${column.name} · ${column.dtype}`),
  ];
}
