/**
 * @file Reads what the host sends of a node: what is connected to it, and the
 * rows after one of its steps. See `NodeView` and `RowsView` in the contract.
 */

import type {
  FieldView,
  InputView,
  NodeView,
  OriginView,
  RowsView,
  SourceView,
  ValuesView,
} from './contract';
import type { Decode } from './decode';
import {
  DecodeError,
  ROOT,
  readField,
  readList,
  readNullable,
  readRecord,
  readCount,
  readString,
  readTagged,
} from './decode';

/** Reads a column and the short name of its dtype. */
export const readFieldView: Decode<FieldView> = readRecord(['name', 'dtype'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  dtype: readField(fields, path, 'dtype', readString),
}));

/** Reads what supplies an input to the node. */
export const readOrigin: Decode<OriginView> = readTagged<OriginView>('kind', {
  node: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'node',
      name: readField(fields, path, 'name', readString),
    }),
  },
  column: {
    fields: ['node', 'column'],
    read: (fields, path) => ({
      kind: 'column',
      node: readField(fields, path, 'node', readString),
      column: readField(fields, path, 'column', readString),
    }),
  },
  parameter: { fields: [], read: () => ({ kind: 'parameter' }) },
});

/** Reads the first values of a column or a list, and how many more follow them. */
export const readValues: Decode<ValuesView> = readRecord(['shown', 'more'], (fields, path) => ({
  shown: readField(fields, path, 'shown', readList(readNullable(readString))),
  more: readField(fields, path, 'more', readCount),
}));

const readSource: Decode<SourceView> = readRecord(['name', 'from', 'columns'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  from: readField(fields, path, 'from', readOrigin),
  columns: readField(fields, path, 'columns', readList(readFieldView)),
}));

const readInput: Decode<InputView> = readTagged<InputView>('kind', {
  table: {
    fields: ['name', 'from', 'columns'],
    read: (fields, path) => ({
      kind: 'table',
      name: readField(fields, path, 'name', readString),
      from: readField(fields, path, 'from', readOrigin),
      columns: readField(fields, path, 'columns', readList(readFieldView)),
    }),
  },
  value: {
    fields: ['name', 'from', 'dtype', 'value'],
    read: (fields, path) => ({
      kind: 'value',
      name: readField(fields, path, 'name', readString),
      from: readField(fields, path, 'from', readOrigin),
      dtype: readField(fields, path, 'dtype', readString),
      value: readField(fields, path, 'value', readNullable(readString)),
    }),
  },
  list: {
    fields: ['name', 'from', 'dtype', 'values'],
    read: (fields, path) => ({
      kind: 'list',
      name: readField(fields, path, 'name', readString),
      from: readField(fields, path, 'from', readOrigin),
      dtype: readField(fields, path, 'dtype', readString),
      values: readField(fields, path, 'values', readValues),
    }),
  },
});

const readNode: Decode<NodeView> = readRecord(['source', 'inputs'], (fields, path) => ({
  source: readField(fields, path, 'source', readSource),
  inputs: readField(fields, path, 'inputs', readList(readInput)),
}));

const readRows: Decode<RowsView> = readRecord(['columns', 'rows'], (fields, path) => {
  const columns = readField(fields, path, 'columns', readList(readFieldView));
  const rows = readField(fields, path, 'rows', readList(readList(readNullable(readString))));
  rows.forEach((row, index) => {
    if (row.length !== columns.length) {
      throw new DecodeError(
        `${path}.rows[${String(index)}]`,
        `must have a cell for each of the ${String(columns.length)} columns, not ${String(row.length)}`,
      );
    }
  });
  return { columns, rows };
});

/**
 * Reads what the host's `node_inputs` command sent.
 *
 * @throws {DecodeError} When it is not a node's inputs, saying where and why.
 */
export function decodeNode(value: unknown): NodeView {
  return readNode(value, ROOT);
}

/**
 * Reads what the host's `rows_after_step` command sent.
 *
 * @throws {DecodeError} When it is not rows, or a row lacks a cell for a column,
 *   saying where and why.
 */
export function decodeRows(value: unknown): RowsView {
  return readRows(value, ROOT);
}
