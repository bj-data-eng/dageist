import { describe, expect, it } from 'vitest';

import { decodeGraph } from '@dageist/host';

import graphFixture from '../../host/fixtures/graph.json';
import { NEW_INPUT_PIN, canvasGraph, connectEdit, guessedHeight, placesOf } from './graph';

// What the host writes of the graph the application opens on.
const graph = canvasGraph(decodeGraph(graphFixture));

describe('the graph the application opens on', () => {
  it('has one box for each node, each after the boxes it reads', () => {
    expect(graph.boxes.map((box) => box.id)).toEqual([
      'claims',
      'holiday-calendar',
      'parameters',
      'sla-by-step',
      'step-names',
      'claim-timing',
      'claim-timing-output',
    ]);
  });

  it('gives a Parameters node an output for each value', () => {
    const parameters = graph.boxes.find((box) => box.kind === 'parameters');
    expect(parameters?.outputs).toEqual([
      { kind: 'value', pin: 'parameter:as_of', name: 'as_of', dtype: 'Date', text: '2026-03-02' },
      { kind: 'value', pin: 'parameter:sla_cutoff', name: 'sla_cutoff', dtype: 'Int64', text: '1' },
    ]);
  });

  it('gives the transform a pin for the table it starts from and for each input', () => {
    const transform = graph.boxes.find((box) => box.id === 'claim-timing');
    expect(transform?.slots.map((slot) => [slot.pin, slot.label, slot.takes])).toEqual([
      ['start', 'starts from', 'table'],
      ['input:as_of', 'as_of', 'value'],
      ['input:holidays', 'holidays', 'list'],
      ['input:sla_cutoff', 'sla_cutoff', 'value'],
      ['input:step_names', 'step_names', 'table'],
      ['input:step_sla', 'step_sla', 'table'],
    ]);
  });

  it('draws a wire for each reference, a list from its table with the column it reads', () => {
    expect(graph.wires).toHaveLength(7);
    expect(graph.wires).toContainEqual({
      id: 'holiday-calendar/table->claim-timing/input:holidays',
      source: 'holiday-calendar',
      sourcePin: 'table',
      target: 'claim-timing',
      targetPin: 'input:holidays',
      column: 'date',
    });
    expect(graph.wires).toContainEqual({
      id: 'parameters/parameter:as_of->claim-timing/input:as_of',
      source: 'parameters',
      sourcePin: 'parameter:as_of',
      target: 'claim-timing',
      targetPin: 'input:as_of',
      column: null,
    });
  });
});

describe('a dragged connection', () => {
  it('connects a table to a table slot the host says it fits', () => {
    expect(
      connectEdit(graph, {
        source: 'step-names',
        sourcePin: 'table',
        target: 'claim-timing',
        targetPin: 'start',
      }),
    ).toEqual({
      edit: 'connect',
      node: 'claim-timing',
      slot: { kind: 'start' },
      from: { kind: 'table', node: 'step-names' },
    });
  });

  it('connects a parameter to a value slot, and not to a table slot', () => {
    const value = {
      source: 'parameters',
      sourcePin: 'parameter:sla_cutoff',
      target: 'claim-timing',
      targetPin: 'input:as_of',
    };
    expect(connectEdit(graph, value)).toMatchObject({ edit: 'connect' });
    expect(connectEdit(graph, { ...value, targetPin: 'start' })).toBeNull();
  });

  it('does not connect a table to a list, which is read from a column', () => {
    expect(
      connectEdit(graph, {
        source: 'holiday-calendar',
        sourcePin: 'table',
        target: 'claim-timing',
        targetPin: 'input:holidays',
      }),
    ).toBeNull();
  });

  it('does not connect what the host did not offer, such as the transform to its own source', () => {
    expect(
      connectEdit(graph, {
        source: 'claim-timing',
        sourcePin: 'table',
        target: 'claim-timing',
        targetPin: NEW_INPUT_PIN,
      }),
    ).toBeNull();
  });

  it('adds an input when a wire is dropped on the new-input pin', () => {
    expect(
      connectEdit(graph, {
        source: 'step-names',
        sourcePin: 'table',
        target: 'claim-timing',
        targetPin: NEW_INPUT_PIN,
      }),
    ).toEqual({
      edit: 'connectNew',
      node: 'claim-timing',
      from: { kind: 'table', node: 'step-names' },
    });
  });

  it('is nothing while either end is missing', () => {
    expect(
      connectEdit(graph, { source: null, sourcePin: null, target: 'claims', targetPin: 'start' }),
    ).toBeNull();
  });
});

describe('where the boxes stand', () => {
  it('puts each box no further left than the boxes it reads, in columns by depth', () => {
    const places = placesOf(graph, () => 100);
    expect(places.get('claims')).toEqual({ x: 0, y: 0 });
    expect(places.get('holiday-calendar')).toEqual({ x: 0, y: 132 });
    expect(places.get('claim-timing')).toEqual({ x: 400, y: 0 });
    expect(places.get('claim-timing-output')).toEqual({ x: 800, y: 0 });
  });

  it('keeps a box where the host placed it', () => {
    const view = decodeGraph(graphFixture);
    const placed = canvasGraph({
      ...view,
      nodes: view.nodes.map((node) =>
        node.id === 'claims'
          ? { ...node, frame: { x: -40, y: 12, width: null, height: null } }
          : node,
      ),
    });
    expect(placesOf(placed, () => 100).get('claims')).toEqual({ x: -40, y: 12 });
  });

  it('guesses a taller box for more lines', () => {
    const [claims, calendar] = graph.boxes;
    expect(claims !== undefined && calendar !== undefined).toBe(true);
    if (claims === undefined || calendar === undefined) return;
    expect(guessedHeight(claims)).toBeGreaterThan(guessedHeight(calendar));
  });
});
