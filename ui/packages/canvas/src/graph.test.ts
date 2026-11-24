import { describe, expect, it } from 'vitest';

import { decodeNode } from '@dageist/host';

import nodeFixture from '../../host/fixtures/node-inputs.json';
import { canvasGraph, guessedHeights, placesOf } from './graph';

// What the host writes of the claim-timing node.
const graph = canvasGraph(decodeNode(nodeFixture));

describe('the graph of the claim-timing node', () => {
  it('has one box for each node that supplies it and one for the parameters', () => {
    expect(graph.suppliers.map((supplier) => supplier.title)).toEqual([
      'Claims',
      'Parameters',
      'Holiday calendar',
      'Step names',
      'SLA by step',
    ]);
  });

  it('gives the parameters one output for each value set on the graph', () => {
    const parameters = graph.suppliers.find((supplier) => supplier.kind === 'parameters');
    expect(parameters?.outputs.map((output) => output.gives)).toEqual([
      { kind: 'parameter', name: 'as_of', dtype: 'Date', value: '2026-03-02' },
      { kind: 'parameter', name: 'sla_cutoff', dtype: 'Int64', value: '1' },
    ]);
  });

  it('gives a node that supplies one column an output for that column', () => {
    const calendar = graph.suppliers.find((supplier) => supplier.title === 'Holiday calendar');
    expect(calendar?.outputs).toEqual([
      { id: 'column:date', gives: { kind: 'column', column: 'date', dtype: 'Date' } },
    ]);
  });

  it('gives a node that supplies its table the columns of that table', () => {
    const claims = graph.suppliers.find((supplier) => supplier.title === 'Claims');
    expect(claims?.outputs.map((output) => output.id)).toEqual(['table']);
    const gives = claims?.outputs[0]?.gives;
    expect(gives?.kind === 'table' ? gives.columns.map((column) => column.name) : []).toEqual([
      'dcn',
      'timestamp',
      'step_to',
      'owner',
      'row_days',
    ]);
  });

  it('lists the table the steps start from first, then every other input', () => {
    expect(graph.inputs).toEqual([
      { name: 'claims', takes: 'table · 5 columns' },
      { name: 'as_of', takes: 'Date' },
      { name: 'holidays', takes: 'list of Date' },
      { name: 'sla_cutoff', takes: 'Int64' },
      { name: 'step_names', takes: 'table · 1 column' },
      { name: 'step_sla', takes: 'table · 2 columns' },
    ]);
  });

  it('wires each input from the output that feeds it', () => {
    expect(graph.wires).toEqual([
      { supplier: 'node:Claims', output: 'table', input: 'claims' },
      { supplier: 'parameters:Parameters', output: 'parameter:as_of', input: 'as_of' },
      { supplier: 'node:Holiday calendar', output: 'column:date', input: 'holidays' },
      { supplier: 'parameters:Parameters', output: 'parameter:sla_cutoff', input: 'sla_cutoff' },
      { supplier: 'node:Step names', output: 'table', input: 'step_names' },
      { supplier: 'node:SLA by step', output: 'table', input: 'step_sla' },
    ]);
  });
});

describe('where the boxes stand', () => {
  it('stacks the suppliers by height with a gap and levels the node with their middle', () => {
    const places = placesOf(graph, { supplier: () => 100, node: 200 });
    expect(graph.suppliers.map((supplier) => places.suppliers.get(supplier.id))).toEqual([
      { x: 0, y: 0 },
      { x: 0, y: 128 },
      { x: 0, y: 256 },
      { x: 0, y: 384 },
      { x: 0, y: 512 },
    ]);
    // The column is five boxes of 100 and four gaps of 28: 612 high.
    expect(places.node).toEqual({ x: 440, y: 206 });
  });

  it('puts the node at the top when it is taller than the column of suppliers', () => {
    expect(placesOf(graph, { supplier: () => 10, node: 1000 }).node.y).toBe(0);
  });

  it('guesses a taller box for a supplier that lists more lines', () => {
    const guess = guessedHeights(graph);
    // Claims lists its table and five columns; the holiday calendar lists one column.
    expect(guess.supplier('node:Claims')).toBeGreaterThan(guess.supplier('node:Holiday calendar'));
  });
});
