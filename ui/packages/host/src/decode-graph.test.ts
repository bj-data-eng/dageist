import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeChosenFile, decodeGraph, decodeGraphEditAnswer, decodePlaced } from './decode-graph';
import graphFixture from '../fixtures/graph.json';
import { replaced, without } from '../test-support/tamper';

// The fixture is written from Rust by the host's snapshot test, from the graph the application
// opens on, with each file's folder written as `/sample`. It is read as `unknown`, as the host's
// answer is, and the decoder is what says what it is.
const graph: unknown = graphFixture;

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

// The ids the host gives the starting graph's nodes, in the order it adds them.
const CLAIMS = '00000000-0000-7000-8000-000000000001';
const SLA = '00000000-0000-7000-8000-000000000002';
const NAMES = '00000000-0000-7000-8000-000000000003';
const CALENDAR = '00000000-0000-7000-8000-000000000004';
const PARAMETERS = '00000000-0000-7000-8000-000000000005';
const TIMING = '00000000-0000-7000-8000-000000000006';
const OUTPUT = '00000000-0000-7000-8000-000000000007';

describe('the graph the application opens on', () => {
  it('lists every node after the nodes it reads', () => {
    const decoded = decodeGraph(graph);
    expect(decoded.nodes.map((node) => [node.id, node.kind])).toEqual([
      [CLAIMS, 'source'],
      [SLA, 'source'],
      [NAMES, 'source'],
      [CALENDAR, 'source'],
      [PARAMETERS, 'parameters'],
      [TIMING, 'transform'],
      [OUTPUT, 'output'],
    ]);
    expect(decoded.parameterDtypes).toEqual(['Int64', 'Float64', 'Boolean', 'String', 'Date']);
  });

  it('gives a source its file and the columns it reads', () => {
    const claims = decodeGraph(graph).nodes[0];
    expect(claims).toMatchObject({
      name: 'Claims',
      summary: 'claims.parquet',
      slots: [],
      newInput: null,
      file: { path: '/sample/claims.parquet', format: 'parquet' },
      problems: [],
      frame: null,
    });
    expect(claims?.columns?.map((column) => column.name)).toEqual([
      'dcn',
      'timestamp',
      'step_to',
      'owner',
      'row_days',
    ]);
  });

  it('gives the transform its slots, what fills each and what could', () => {
    const transform = decodeGraph(graph).nodes.find((node) => node.id === TIMING);
    expect(transform?.summary).toBe('12 steps');
    expect(transform?.slots.map((slot) => [slot.slot, slot.takes, slot.from])).toEqual([
      [{ kind: 'start' }, 'table', { kind: 'table', node: CLAIMS }],
      [
        { kind: 'input', name: 'as_of' },
        'value',
        { kind: 'parameter', node: PARAMETERS, name: 'as_of' },
      ],
      [
        { kind: 'input', name: 'holidays' },
        'list',
        { kind: 'column', node: CALENDAR, column: 'date' },
      ],
      [
        { kind: 'input', name: 'sla_cutoff' },
        'value',
        { kind: 'parameter', node: PARAMETERS, name: 'sla_cutoff' },
      ],
      [{ kind: 'input', name: 'step_names' }, 'table', { kind: 'table', node: NAMES }],
      [{ kind: 'input', name: 'step_sla' }, 'table', { kind: 'table', node: SLA }],
    ]);
    expect(transform?.slots[1]?.fits).toEqual([
      { kind: 'parameter', node: PARAMETERS, name: 'as_of' },
      { kind: 'parameter', node: PARAMETERS, name: 'sla_cutoff' },
    ]);
    expect(transform?.newInput).toHaveLength(6);
  });

  it('says what an output still lacks', () => {
    const output = decodeGraph(graph).nodes.at(-1);
    expect(output?.problems).toEqual(['no file is chosen']);
    expect(output?.file).toBeNull();
  });

  it('refuses a node of a kind the contract lacks, saying where', () => {
    const tampered = replaced(graph, ['nodes', 0, 'kind'], 'sink');
    expect(refusal(decodeGraph, tampered).message).toContain('$.nodes[0].kind');
  });

  it('refuses a node without its problems', () => {
    expect(refusal(decodeGraph, without(graph, ['nodes', 6], 'problems')).message).toContain(
      'problems',
    );
  });

  it('refuses a box placed at a fraction', () => {
    const tampered = replaced(graph, ['nodes', 0, 'frame'], {
      x: 1.5,
      y: 0,
      width: null,
      height: null,
    });
    expect(refusal(decodeGraph, tampered).message).toContain('$.nodes[0].frame.x');
  });
});

describe('the answer to an edit of the graph', () => {
  it('is the edited graph with the node added, or the refusal with the revision kept', () => {
    const edited = decodeGraphEditAnswer({ kind: 'edited', graph, added: 'transform' });
    expect(edited.kind === 'edited' && edited.added).toBe('transform');
    expect(
      decodeGraphEditAnswer({ kind: 'refused', revision: 23, message: 'there is no node x' }),
    ).toEqual({ kind: 'refused', revision: 23, message: 'there is no node x' });
  });

  it('refuses an answer of a kind the contract lacks', () => {
    expect(() => decodeGraphEditAnswer({ kind: 'maybe' })).toThrow(DecodeError);
  });
});

describe('the layout revision and the file chosen', () => {
  it('reads the revision a placed box moved the layout to', () => {
    expect(decodePlaced({ layoutRevision: 4 })).toEqual({ layoutRevision: 4 });
    expect(() => decodePlaced({ layoutRevision: -1 })).toThrow(DecodeError);
  });

  it('reads a chosen file, or nothing chosen', () => {
    expect(decodeChosenFile(null)).toBeNull();
    expect(decodeChosenFile({ path: '/data/a.csv', format: 'csv' })).toEqual({
      path: '/data/a.csv',
      format: 'csv',
    });
    expect(() => decodeChosenFile({ path: '/data/a.xlsx', format: 'excel' })).toThrow(DecodeError);
  });
});
