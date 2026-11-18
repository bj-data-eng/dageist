/**
 * @file Reads what the host sends of the core's identity and of the sample
 * table. See `IdentityView`, `TableView` and `fetchTableBlock` in the
 * contract.
 */

import type { ColumnView, FiguresView, IdentityView, TableView } from './contract';
import type { Decode } from './decode';
import {
  ROOT,
  readBoolean,
  readCount,
  readField,
  readList,
  readRecord,
  readString,
} from './decode';

const readIdentity: Decode<IdentityView> = readRecord(['name', 'version'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  version: readField(fields, path, 'version', readString),
}));

const readColumn: Decode<ColumnView> = readRecord(
  ['name', 'dataType', 'numeric'],
  (fields, path) => ({
    name: readField(fields, path, 'name', readString),
    dataType: readField(fields, path, 'dataType', readString),
    numeric: readField(fields, path, 'numeric', readBoolean),
  }),
);

const readFigures: Decode<FiguresView> = readRecord(
  ['rows', 'columns', 'opened'],
  (fields, path) => ({
    rows: readField(fields, path, 'rows', readString),
    columns: readField(fields, path, 'columns', readString),
    opened: readField(fields, path, 'opened', readString),
  }),
);

const readTable: Decode<TableView> = readRecord(['rows', 'columns', 'figures'], (fields, path) => ({
  rows: readField(fields, path, 'rows', readCount),
  columns: readField(fields, path, 'columns', readList(readColumn)),
  figures: readField(fields, path, 'figures', readFigures),
}));

/**
 * Reads what the host's `core_identity` command sent.
 *
 * @throws {DecodeError} When it is not an identity, saying where and why.
 */
export function decodeIdentity(value: unknown): IdentityView {
  return readIdentity(value, ROOT);
}

/**
 * Reads what the host's `open_sample_window` and `open_sample_result` commands
 * sent: the shape of the table that was opened, with its figures as text.
 *
 * @throws {DecodeError} When it is not a table's shape, saying where and why.
 */
export function decodeTable(value: unknown): TableView {
  return readTable(value, ROOT);
}
