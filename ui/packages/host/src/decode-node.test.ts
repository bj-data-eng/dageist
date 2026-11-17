import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeNode, decodeRows } from './decode-node';
import nodeFixture from '../fixtures/node-inputs.json';
import rowsFixture from '../fixtures/step-rows.json';
import { replaced, withField, without } from '../test-support/tamper';

// The fixtures are written from Rust by the host's snapshot test. They are read as `unknown`,
// as the host's answer is, and the decoder is what says what they are.
const node: unknown = nodeFixture;
const rows: unknown = rowsFixture;

/** What decoding `value` with `decode` was refused with. */
function refusal(decode: (value: unknown) => unknown, value: unknown): DecodeError {
  try {
    decode(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

describe('the inputs of the claim-timing node', () => {
  it('are the claims table the steps start from and each other input by name', () => {
    const decoded = decodeNode(node);
    expect(decoded.source).toEqual({
      name: 'claims',
      columns: [
        { name: 'dcn', dtype: 'String' },
        { name: 'timestamp', dtype: 'Datetime(us)' },
        { name: 'step_to', dtype: 'String' },
        { name: 'owner', dtype: 'String' },
        { name: 'row_days', dtype: 'Int64' },
      ],
    });
    expect(decoded.inputs).toEqual([
      { kind: 'value', name: 'as_of', dtype: 'Date' },
      { kind: 'list', name: 'holidays', dtype: 'Date' },
      { kind: 'value', name: 'sla_cutoff', dtype: 'Int64' },
      { kind: 'table', name: 'step_names', columns: [{ name: 'step_to', dtype: 'String' }] },
      {
        kind: 'table',
        name: 'step_sla',
        columns: [
          { name: 'step', dtype: 'String' },
          { name: 'sla_days', dtype: 'Int64' },
        ],
      },
    ]);
  });

  it('are refused with the place of what is wrong', () => {
    expect(refusal(decodeNode, replaced(node, ['inputs', 1, 'kind'], 'set')).message).toBe(
      '$.inputs[1].kind: must be one of "table", "value", "list", not "set"',
    );
    expect(refusal(decodeNode, without(node, ['source'], 'name')).message).toBe(
      '$.source: is missing the field "name"',
    );
    expect(refusal(decodeNode, replaced(node, ['source', 'columns', 0, 'dtype'], 7)).message).toBe(
      '$.source.columns[0].dtype: must be a string, not a number',
    );
    // A list has a dtype, not columns.
    expect(refusal(decodeNode, withField(node, ['inputs', 1], 'columns', [])).message).toBe(
      '$.inputs[1]: has the field "columns", which the contract lacks',
    );
    expect(refusal(decodeNode, null).message).toBe('$: must be an object, not null');
  });
});

describe('the rows after a step', () => {
  it('are text for each cell, and null where the value is', () => {
    const decoded = decodeRows(rows);
    expect(decoded.columns.map((column) => column.name)).toEqual([
      'dcn',
      'timestamp',
      'step_to',
      'owner',
      'row_days',
      'step_index',
      'resolved_at',
      'received_at',
      'resolved_day',
      'days_open',
    ]);
    expect(decoded.columns.map((column) => column.dtype)).toEqual([
      'String',
      'Datetime(us)',
      'String',
      'String',
      'Int64',
      'UInt32',
      'Datetime(us)',
      'Datetime(us)',
      'Date',
      'Int64',
    ]);
    expect(decoded.rows).toHaveLength(7);
    // The first claim's first row has no row before it, so what is carried over is null.
    expect(decoded.rows[0]).toEqual([
      'A-1001',
      '2026-03-02 09:15:00',
      'Intake',
      'Company A',
      '0',
      '1',
      null,
      '2026-03-02 09:15:00',
      null,
      null,
    ]);
    expect(decoded.rows[1]).toEqual([
      'A-1001',
      '2026-03-03 16:40:00',
      'Review',
      'Company B',
      '1',
      '2',
      '2026-03-02 09:15:00',
      '2026-03-03 16:40:00',
      '2026-03-02',
      '1',
    ]);
  });

  it('are refused for a cell that is not text or null', () => {
    expect(refusal(decodeRows, replaced(rows, ['rows', 0, 1], 5)).message).toBe(
      '$.rows[0][1]: must be a string, not a number',
    );
    expect(refusal(decodeRows, replaced(rows, ['rows', 3, 9], false)).message).toBe(
      '$.rows[3][9]: must be a string, not a boolean',
    );
  });

  it('are refused for a row that lacks a cell for a column', () => {
    const short = replaced(rows, ['rows', 2], ['A-1001', null]);
    expect(refusal(decodeRows, short).message).toBe(
      '$.rows[2]: must have a cell for each of the 10 columns, not 2',
    );
  });

  it('are refused for columns that are not fields of a name and a dtype', () => {
    expect(refusal(decodeRows, without(rows, ['columns', 0], 'dtype')).message).toBe(
      '$.columns[0]: is missing the field "dtype"',
    );
    expect(refusal(decodeRows, replaced(rows, ['rows'], 'none')).message).toBe(
      '$.rows: must be a list, not a string',
    );
  });
});
