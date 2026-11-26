import { tableFromArrays, tableToIPC, utf8 } from '@uwdata/flechette';
import type { Table } from '@uwdata/flechette';
import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeBlock } from './decode-block';
import resultFixture from '../fixtures/table-block-result.json';
import windowFixture from '../fixtures/table-block-window.json';
import { readBlockFixture } from '../test-support/block-fixture';

// The fixtures are the frames the host answered, written from Rust by its snapshot test. What
// they hold is written out here by hand, from the cells read in Python Polars.

/** What decoding `value` for the columns `names` was refused with. */
function refusal(value: unknown, names: readonly string[]): DecodeError {
  try {
    decodeBlock(value, names);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

/** The bytes of a frame with `header` as its header, and `arrow` after it. */
function frameOf(header: unknown, arrow: Uint8Array): ArrayBuffer {
  const text =
    header instanceof Uint8Array ? header : new TextEncoder().encode(JSON.stringify(header));
  const bytes = new Uint8Array(4 + text.length + arrow.length);
  new DataView(bytes.buffer).setUint32(0, text.length);
  bytes.set(text, 4);
  bytes.set(arrow, 4 + text.length);
  return bytes.buffer;
}

/** An Arrow stream of the text columns `columns`. A column is a list of cells, `null` for a missing one. */
function textStream(columns: Record<string, Array<string | null>>): Uint8Array {
  const types = Object.fromEntries(Object.keys(columns).map((name) => [name, utf8()]));
  return streamOf(tableFromArrays(columns, { types }));
}

function streamOf(table: Table): Uint8Array {
  const bytes = tableToIPC(table, { format: 'stream' });
  if (bytes === null) throw new Error('the table was not written');
  return bytes;
}

describe('a block of a working window', () => {
  const { request, frame } = readBlockFixture(windowFixture);

  it('is the numbers of its rows, its report and its cells, all as the host wrote them', () => {
    const block = decodeBlock(frame, request.columns);

    expect(block.numbers).toEqual(['4', '5', '6', '7', '8', '9', '10']);
    expect(block.report).toBe('7 rows × 5 columns · 1.6 KB · built in 850 µs');
    expect(block.columns).toHaveLength(5);
    expect(block.columns[0]).toEqual([
      'DOC-10000000',
      'DOC-10000000',
      'DOC-10000000',
      'DOC-10000000',
      'DOC-10000001',
      'DOC-10000001',
      'DOC-10000001',
    ]);
    expect(block.columns[1]?.[0]).toBe('2025-06-07 09:50:15');
    expect(block.columns[1]?.[6]).toBe('2025-07-31 22:09:51');
    expect(block.columns[2]?.[4]).toBe('2025-07-29');
    expect(block.columns[3]).toEqual([
      '485.39',
      '884.42',
      '792.18',
      '916.51',
      '42.55',
      '740.05',
      '532.54',
    ]);
    expect(block.columns[4]?.[0]).toBe('55578');
  });

  it('asks for the columns in the order they were asked for', () => {
    expect(request.columns).toEqual([
      'document_id',
      'timestamp',
      'date_of_receipt',
      'measure_001',
      'measure_003',
    ]);
  });
});

describe('a block of a stored result', () => {
  it('has a cell for each number, a row that has no tenths as the host wrote it', () => {
    const { request, frame } = readBlockFixture(resultFixture);

    const block = decodeBlock(frame, request.columns);

    expect(block.numbers).toEqual(['79', '80', '81', '82', '83', '84', '85', '86', '87', '88']);
    expect(block.report).toBe('10 rows × 5 columns · 1.9 KB · built in 850 µs');
    expect(block.columns.every((cells) => cells.length === 10)).toBe(true);
    expect(block.columns[3]?.[9]).toBe('5.7');
    expect(block.columns[4]?.[8]).toBe('90584');
  });
});

describe('a block of every column', () => {
  it('is read for no columns asked for, which the host answers with all of them', () => {
    const { request, frame } = readBlockFixture(windowFixture);

    const all = decodeBlock(frame, []);

    expect(all).toEqual(decodeBlock(frame, request.columns));
  });

  it('holds the columns as the stream has them, a missing cell as null', () => {
    const frame = frameOf(
      { numbers: ['1', '2'], report: 'a report' },
      textStream({ b: ['x', null], a: ['y', 'z'] }),
    );

    expect(decodeBlock(frame, []).columns).toEqual([
      ['x', null],
      ['y', 'z'],
    ]);
  });

  it('is still refused when it is not text, or has other rows than the header has numbers', () => {
    const numbers = streamOf(tableFromArrays({ a: [1, 2, 3] }));

    expect(refusal(frameOf({ numbers: ['1', '2', '3'], report: 'r' }, numbers), []).path).toBe(
      '$.arrow.columns[0]',
    );
    expect(
      refusal(frameOf({ numbers: ['1'], report: 'r' }, textStream({ a: ['x', 'y'] })), []).message,
    ).toBe('$.arrow: has 2 rows, and the header numbers 1');
  });
});

describe('a block with a missing cell', () => {
  it('holds null where a cell is missing, and the text of the others as it is', () => {
    const frame = frameOf(
      { numbers: ['1', '2', '3'], report: 'a report' },
      textStream({ a: ['x', null, ''], b: [null, null, 'null'] }),
    );

    const block = decodeBlock(frame, ['a', 'b']);

    expect(block.columns).toEqual([
      ['x', null, ''],
      [null, null, 'null'],
    ]);
  });

  it('is a block of no rows when the header has no numbers', () => {
    const frame = frameOf({ numbers: [], report: '0 rows × 1 column' }, textStream({ a: [] }));

    expect(decodeBlock(frame, ['a'])).toEqual({
      numbers: [],
      report: '0 rows × 1 column',
      columns: [[]],
    });
  });
});

describe('a block of the wrong shape', () => {
  const { request, frame } = readBlockFixture(windowFixture);
  const header = { numbers: ['1'], report: 'a report' };

  it('is refused when it is not bytes, or too short to give the length of its header', () => {
    expect(refusal('bytes', ['a']).message).toBe('$: must be the bytes of a block');
    expect(refusal(new Uint8Array([1, 2, 3, 4]), ['a']).message).toBe(
      '$: must be the bytes of a block',
    );
    expect(refusal(new Uint8Array([0, 0, 0]).buffer, ['a']).message).toBe(
      '$: is too short to start with the length of its header',
    );
  });

  it('is refused when its header is longer than the frame, as an Arrow stream alone is', () => {
    const arrow = textStream({ a: ['x'] });

    expect(refusal(arrow.slice().buffer, ['a']).problem).toBe(
      'has a header of 4294967295 bytes, which is more than the ' +
        `${String(arrow.length - 4)} bytes after its length`,
    );
  });

  it('is refused when its header is not UTF-8 text or not JSON', () => {
    const arrow = textStream({ a: ['x'] });

    expect(refusal(frameOf(new Uint8Array([0xff, 0xfe]), arrow), ['a']).message).toMatch(
      /^\$\.header: is not UTF-8 text/u,
    );
    expect(refusal(frameOf(new TextEncoder().encode('{"numbers"'), arrow), ['a']).message).toMatch(
      /^\$\.header: is not JSON/u,
    );
  });

  it('is refused when its header lacks a field, has another, or holds the wrong kind', () => {
    const arrow = textStream({ a: ['x'] });

    expect(refusal(frameOf({ numbers: ['1'] }, arrow), ['a']).message).toBe(
      '$.header: is missing the field "report"',
    );
    expect(refusal(frameOf({ ...header, rows: 1 }, arrow), ['a']).message).toBe(
      '$.header: has the field "rows", which the contract lacks',
    );
    expect(refusal(frameOf({ numbers: [1], report: 'r' }, arrow), ['a']).message).toBe(
      '$.header.numbers[0]: must be a string, not a number',
    );
    expect(refusal(frameOf(['1'], arrow), ['a']).message).toBe(
      '$.header: must be an object, not a list',
    );
  });

  it('is refused when what follows the header is not an Arrow stream', () => {
    const refused = refusal(frameOf(header, new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8])), ['a']);

    expect(refused.path).toBe('$.arrow');
    expect(refused.problem).toMatch(/^is not an Arrow stream/u);
  });

  it('is refused when it has other columns than were asked for', () => {
    expect(refusal(frame, request.columns.slice(1)).message).toBe(
      '$.arrow: has 5 columns, not the 4 asked for',
    );
    const reversed = ['measure_003', 'measure_001', 'date_of_receipt', 'timestamp', 'document_id'];
    expect(refusal(frame, reversed).message).toBe(
      '$.arrow.columns[0]: is the column "document_id", not "measure_003" as asked for',
    );
  });

  it('is refused when a column is not text', () => {
    const numbers = streamOf(tableFromArrays({ a: [1, 2, 3] }));

    const refused = refusal(frameOf({ numbers: ['1', '2', '3'], report: 'r' }, numbers), ['a']);

    expect(refused.path).toBe('$.arrow.columns[0]');
    expect(refused.problem).toMatch(/^must be text, not of the Arrow type with the id /u);
  });

  it('is refused when it has other rows than the header has numbers', () => {
    const arrow = textStream({ a: ['x', 'y'] });

    expect(refusal(frameOf(header, arrow), ['a']).message).toBe(
      '$.arrow: has 2 rows, and the header numbers 1',
    );
  });
});
