/**
 * @file Test support: reads `table-block-window.json` and `table-block-result.json`,
 * which the host's tests write from the block command: what was asked for and the
 * frame the host answered, as hexadecimal text. It is not something the host
 * sends, so it is not part of the package's front door.
 */

import type { BlockRequest } from '../src/contract';
import type { Decode } from '../src/decode';
import {
  DecodeError,
  ROOT,
  readCount,
  readField,
  readList,
  readRecord,
  readString,
} from '../src/decode';

/** A request for a block and the frame the host answered it with. */
export interface BlockFixture {
  readonly request: BlockRequest;
  readonly frame: ArrayBuffer;
}

const readRequest: Decode<BlockRequest> = readRecord(
  ['offset', 'length', 'columns'],
  (fields, path) => ({
    offset: readField(fields, path, 'offset', readCount),
    length: readField(fields, path, 'length', readCount),
    columns: readField(fields, path, 'columns', readList(readString)),
  }),
);

/** The bytes that `text`, written in hexadecimal, stands for. */
export function bytesOfHex(text: string): ArrayBuffer {
  const pairs = text.match(/[0-9a-f]{2}/gu) ?? [];
  if (pairs.length * 2 !== text.length) throw new DecodeError(ROOT, 'is not hexadecimal text');
  return Uint8Array.from(pairs, (pair) => Number(`0x${pair}`)).buffer;
}

/** Reads a block fixture. */
export function readBlockFixture(value: unknown): BlockFixture {
  return readRecord(['request', 'frame'], (fields, path) => ({
    request: readField(fields, path, 'request', readRequest),
    frame: bytesOfHex(readField(fields, path, 'frame', readString)),
  }))(value, ROOT);
}
