import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeIdentity, decodeTable } from './decode-sample';
import resultFixture from '../fixtures/table-result.json';
import windowFixture from '../fixtures/table-window.json';
import { replaced, withField, without } from '../test-support/tamper';

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
  it('is its row count, its columns and its figures as the host wrote them', () => {
    const window = decodeTable(windowFixture);
    expect(window.rows).toBe(46);
    expect(window).toMatchObject({
      figures: { rows: '46 rows', columns: '12 columns', opened: '12.0 ms' },
    });
    expect(window.columns).toHaveLength(12);
    expect(window.columns[0]).toEqual({ name: 'dcn', dataType: 'str', numeric: false });
    expect(window.columns[1]).toEqual({
      name: 'timestamp',
      dataType: 'datetime[μs]',
      numeric: false,
    });
    expect(window.columns[8]).toEqual({ name: 'measure_001', dataType: 'f64', numeric: true });
    expect(window.columns[9]).toEqual({ name: 'measure_002', dataType: 'i32', numeric: true });

    const result = decodeTable(resultFixture);
    expect(result.rows).toBe(192);
    expect(result).toMatchObject({
      figures: { rows: '192 rows', columns: '12 columns', opened: '12.0 ms' },
    });
  });

  it('is an empty table with no columns', () => {
    const empty = {
      rows: 0,
      columns: [],
      figures: { rows: '0 rows', columns: '0 columns', opened: '0 µs' },
    };
    expect(decodeTable(empty)).toEqual(empty);
  });

  it('is refused for a row count that is not a whole number from 0, and a malformed column', () => {
    expect(refusal(decodeTable, replaced(windowFixture, ['rows'], -1)).message).toBe(
      '$.rows: must be a whole number from 0, not -1',
    );
    expect(refusal(decodeTable, replaced(windowFixture, ['rows'], '3')).message).toBe(
      '$.rows: must be a whole number, not a string',
    );
    expect(refusal(decodeTable, without(windowFixture, ['columns', 0], 'dataType')).message).toBe(
      '$.columns[0]: is missing the field "dataType"',
    );
    expect(refusal(decodeTable, without(windowFixture, ['columns', 0], 'numeric')).message).toBe(
      '$.columns[0]: is missing the field "numeric"',
    );
    expect(
      refusal(decodeTable, replaced(windowFixture, ['columns', 0, 'numeric'], 'no')).message,
    ).toBe('$.columns[0].numeric: must be a boolean, not a string');
  });

  it('is refused where the figures are missing, another kind, or more than the contract has', () => {
    expect(refusal(decodeTable, without(windowFixture, [], 'figures')).message).toBe(
      '$: is missing the field "figures"',
    );
    expect(refusal(decodeTable, without(windowFixture, ['figures'], 'opened')).message).toBe(
      '$.figures: is missing the field "opened"',
    );
    expect(refusal(decodeTable, replaced(windowFixture, ['figures', 'rows'], 46)).message).toBe(
      '$.figures.rows: must be a string, not a number',
    );
    expect(
      refusal(decodeTable, withField(windowFixture, ['figures'], 'closed', '1 ms')).message,
    ).toBe('$.figures: has the field "closed", which the contract lacks');
  });
});
