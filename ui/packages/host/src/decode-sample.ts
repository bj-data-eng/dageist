/**
 * @file Reads what the host sends of the core's identity and of the sample
 * table. See `IdentityView`, `TableView` and `fetchTableBlock` in the
 * contract.
 */

import type { ColumnView, IdentityView, TableView } from './contract';
import type { Decode } from './decode';
import {
  DecodeError,
  ROOT,
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

const readColumn: Decode<ColumnView> = readRecord(['name', 'dataType'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  dataType: readField(fields, path, 'dataType', readString),
}));

const readTable: Decode<TableView> = readRecord(['rows', 'columns'], (fields, path) => ({
  rows: readField(fields, path, 'rows', readCount),
  columns: readField(fields, path, 'columns', readList(readColumn)),
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
 * sent: the shape of the table that was opened.
 *
 * @throws {DecodeError} When it is not a table's shape, saying where and why.
 */
export function decodeTable(value: unknown): TableView {
  return readTable(value, ROOT);
}

/**
 * Reads what the host's `table_block` command sent: the bytes of an Arrow IPC
 * stream, which the table viewer decodes itself.
 *
 * @throws {DecodeError} When it is not bytes.
 */
export function decodeBytes(value: unknown): ArrayBuffer {
  if (!(value instanceof ArrayBuffer)) {
    throw new DecodeError(ROOT, 'must be the bytes of an Arrow stream');
  }
  return value;
}
