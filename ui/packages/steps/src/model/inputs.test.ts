import { describe, expect, it } from 'vitest';

import { decodeNode } from '@dageist/host';

import nodeFixture from '../../../host/fixtures/node-inputs.json';
import { findInput, inputLines, originWords, sourceLines } from './inputs';

// What the host writes of the claim-timing node.
const node = decodeNode(nodeFixture);

/** The input of the node called `name`. */
function inputNamed(name: string): NonNullable<ReturnType<typeof findInput>> {
  const found = findInput(node, name);
  if (found === null) throw new Error(`no input ${name}`);
  return found;
}

describe('what supplies an input', () => {
  it('is said as the node, as a column of a node, or as parameters', () => {
    expect(originWords({ kind: 'node', name: 'SLA by step' })).toBe('SLA by step');
    expect(originWords({ kind: 'column', node: 'Holiday calendar', column: 'date' })).toBe(
      'Holiday calendar · column "date"',
    );
    expect(originWords({ kind: 'parameter' })).toBe('Parameters');
  });
});

describe('the lines that describe the node’s inputs', () => {
  it('give a table its source and its columns', () => {
    expect(inputLines(inputNamed('step_sla'))).toEqual([
      'step_sla · table',
      'from SLA by step',
      'step · String',
      'sla_days · Int64',
    ]);
  });

  it('give a value its dtype, its source and the value the engine wrote', () => {
    expect(inputLines(inputNamed('as_of'))).toEqual([
      'as_of · Date',
      'from Parameters',
      '2026-03-02',
    ]);
    expect(inputLines(inputNamed('sla_cutoff'))).toEqual([
      'sla_cutoff · Int64',
      'from Parameters',
      '1',
    ]);
  });

  it('give a list its dtype, its source and its values', () => {
    expect(inputLines(inputNamed('holidays'))).toEqual([
      'holidays · list of Date',
      'from Holiday calendar · column "date"',
      '2026-03-03, 2026-03-05',
    ]);
  });

  it('write a value the engine gave as null, and the values that follow a list’s, as the contract allows', () => {
    expect(
      inputLines({
        kind: 'value',
        name: 'nothing',
        from: { kind: 'parameter' },
        dtype: 'Int64',
        value: null,
      }),
    ).toEqual(['nothing · Int64', 'from Parameters', '—']);
    expect(
      inputLines({
        kind: 'list',
        name: 'many',
        from: { kind: 'node', name: 'Calendar' },
        dtype: 'Int64',
        values: { shown: ['1', null], more: 5 },
      }),
    ).toEqual(['many · list of Int64', 'from Calendar', '1, —, …']);
  });

  it('say the table the steps start from, with where it comes from', () => {
    expect(sourceLines(node)).toEqual([
      'claims · table · the steps start from its rows',
      'from Claims',
      'dcn · String',
      'timestamp · Datetime(us)',
      'step_to · String',
      'owner · String',
      'row_days · Int64',
    ]);
  });
});
