/**
 * @file Reads what the host's `table_block` command sends: a block of the open
 * table as a frame. The frame is four bytes that give the length of the header
 * (a big-endian number), the header as JSON, and an Arrow IPC stream to the
 * end. The header holds the number of each row and a report of the block, both
 * written by the host, and the stream holds the cells, every column text.
 *
 * Nothing is formatted here: a cell is the text the host sent, and a cell
 * that is not text is refused.
 */

import type { Table } from '@uwdata/flechette';
import { Type, tableFromIPC } from '@uwdata/flechette';

import type { BlockView } from './contract';
import type { Decode } from './decode';
import {
  DecodeError,
  ROOT,
  fieldPath,
  itemPath,
  readField,
  readList,
  readRecord,
  readString,
} from './decode';

/** The bytes that give the length of the header. */
const LENGTH_BYTES = 4;
/** The place of the header in the frame. */
const HEADER = fieldPath(ROOT, 'header');
/** The place of the Arrow stream in the frame. */
const ARROW = fieldPath(ROOT, 'arrow');
/** The Arrow types that hold text. */
const TEXT_TYPES: readonly number[] = [Type.Utf8, Type.LargeUtf8, Type.Utf8View];

/** What the header of a frame holds. */
interface Header {
  readonly numbers: readonly string[];
  readonly report: string;
}

const readHeader: Decode<Header> = readRecord(['numbers', 'report'], (fields, path) => ({
  numbers: readField(fields, path, 'numbers', readList(readString)),
  report: readField(fields, path, 'report', readString),
}));

/** What went wrong, as a phrase for a reason. */
function reasonOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Reads the header out of `bytes`, which are the bytes of the frame after the length. */
function parseHeader(bytes: Uint8Array): Header {
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch (error) {
    throw new DecodeError(HEADER, `is not UTF-8 text (${reasonOf(error)})`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new DecodeError(HEADER, `is not JSON (${reasonOf(error)})`);
  }
  return readHeader(parsed, HEADER);
}

/** Reads the Arrow stream `bytes` as a table of text, the columns being `names`. */
function parseTable(bytes: Uint8Array, names: readonly string[]): Table {
  let table: Table;
  try {
    table = tableFromIPC(bytes);
  } catch (error) {
    throw new DecodeError(ARROW, `is not an Arrow stream (${reasonOf(error)})`);
  }
  const fields = table.schema.fields;
  if (fields.length !== names.length) {
    throw new DecodeError(
      ARROW,
      `has ${String(fields.length)} columns, not the ${String(names.length)} asked for`,
    );
  }
  fields.forEach((field, index) => {
    const place = itemPath(fieldPath(ARROW, 'columns'), index);
    if (field.name !== names[index]) {
      throw new DecodeError(
        place,
        `is the column ${JSON.stringify(field.name)}, not ${JSON.stringify(names[index])} as asked for`,
      );
    }
    if (!TEXT_TYPES.includes(field.type.typeId)) {
      throw new DecodeError(
        place,
        `must be text, not of the Arrow type with the id ${String(field.type.typeId)}`,
      );
    }
  });
  return table;
}

/** Reads the cell at `place` as text, or `null` for a missing value. */
function readCell(value: unknown, place: string): string | null {
  if (value === null) return null;
  if (typeof value !== 'string') throw new DecodeError(place, 'must be text or missing');
  return value;
}

/**
 * Reads what the host's `table_block` command sent, when the columns `names`
 * were asked for.
 *
 * @throws {DecodeError} When it is not a frame of a block of those columns:
 * its header is not the header's shape, its Arrow stream cannot be read, has
 * other columns than asked for, has a column that is not text, or has another
 * number of rows than the header has numbers.
 */
export function decodeBlock(value: unknown, names: readonly string[]): BlockView {
  if (!(value instanceof ArrayBuffer)) {
    throw new DecodeError(ROOT, 'must be the bytes of a block');
  }
  if (value.byteLength < LENGTH_BYTES) {
    throw new DecodeError(ROOT, 'is too short to start with the length of its header');
  }
  const length = new DataView(value).getUint32(0);
  const end = LENGTH_BYTES + length;
  if (end > value.byteLength) {
    throw new DecodeError(
      ROOT,
      `has a header of ${String(length)} bytes, which is more than the ${String(value.byteLength - LENGTH_BYTES)} bytes after its length`,
    );
  }
  const header = parseHeader(new Uint8Array(value, LENGTH_BYTES, length));
  // The stream is copied to a buffer of its own: Arrow reads its numbers in place, which needs
  // them at a multiple of their size from the start of the buffer, and the header's length is
  // anything.
  const table = parseTable(new Uint8Array(value.slice(end)), names);
  if (table.numRows !== header.numbers.length) {
    throw new DecodeError(
      ARROW,
      `has ${String(table.numRows)} rows, and the header numbers ${String(header.numbers.length)}`,
    );
  }
  const columns = names.map((name, index) => {
    const cells = table.getChildAt<unknown>(index);
    const place = fieldPath(fieldPath(ARROW, 'columns'), name);
    return Array.from({ length: table.numRows }, (_unused, row) =>
      readCell(cells?.at(row), itemPath(place, row)),
    );
  });
  return { numbers: header.numbers, report: header.report, columns };
}
