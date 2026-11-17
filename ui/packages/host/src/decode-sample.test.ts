import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeBytes, decodeIdentity, decodeTable } from './decode-sample';

function refusal(decode: (value: unknown) => unknown, value: unknown): DecodeError {
  try {
    decode(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

describe('the identity of the core', () => {
  it('is its name and its version', () => {
    expect(decodeIdentity({ name: 'dageist', version: '0.1.0' })).toEqual({
      name: 'dageist',
      version: '0.1.0',
    });
  });

  it('is refused where a field is missing, another kind, or one the contract lacks', () => {
    expect(refusal(decodeIdentity, { name: 'dageist' }).message).toBe(
      '$: is missing the field "version"',
    );
    expect(refusal(decodeIdentity, { name: 'dageist', version: 1 }).message).toBe(
      '$.version: must be a string, not a number',
    );
    expect(refusal(decodeIdentity, { name: 'dageist', version: '0.1.0', build: 3 }).message).toBe(
      '$: has the field "build", which the contract lacks',
    );
  });
});

describe('the shape of an opened table', () => {
  it('is its row count and each column with its dtype as Polars prints it', () => {
    const shape = {
      rows: 2552,
      columns: [
        { name: 'dcn', dataType: 'str' },
        { name: 'measure_001', dataType: 'i32' },
      ],
    };
    expect(decodeTable(shape)).toEqual(shape);
    expect(decodeTable({ rows: 0, columns: [] })).toEqual({ rows: 0, columns: [] });
  });

  it('is refused for a row count that is not a whole number from 0, and a malformed column', () => {
    expect(refusal(decodeTable, { rows: -1, columns: [] }).message).toBe(
      '$.rows: must be a whole number from 0, not -1',
    );
    expect(
      refusal(decodeTable, { rows: 1, columns: [{ name: 'a', data_type: 'str' }] }).message,
    ).toBe('$.columns[0]: is missing the field "dataType"');
    expect(refusal(decodeTable, { rows: '3', columns: [] }).message).toBe(
      '$.rows: must be a whole number, not a string',
    );
  });
});

describe('a block of rows', () => {
  it('is the bytes of an Arrow stream, as they arrive', () => {
    const bytes = new Uint8Array([1, 2, 3]).buffer;
    expect(decodeBytes(bytes)).toBe(bytes);
  });

  it('is refused when it is anything else', () => {
    expect(refusal(decodeBytes, new Uint8Array([1])).message).toBe(
      '$: must be the bytes of an Arrow stream',
    );
    expect(refusal(decodeBytes, 'bytes').problem).toBe('must be the bytes of an Arrow stream');
    expect(refusal(decodeBytes, null).problem).toBe('must be the bytes of an Arrow stream');
  });
});
