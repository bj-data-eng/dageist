/**
 * @file The claim-timing steps the editor opens with. They are the steps of
 * the engine's claim-timing sample: translated into a document they are that
 * sample's own, which a test checks against the document the host writes, so
 * the editor starts from what the engine can run. The words of each number and
 * text are the engine's, which a test checks against how the engine reads them.
 */

import { nextId } from './model/ids';
import { NO_OPTIONS, expressionSetting } from './model/settings';
import type { Chain, Value } from './model/source';
import type { Step } from './model/steps';
import { elementChain, fieldKeys } from './model/steps';

/** Returns `chain` as what a column holds: a field alone is that field, anything more is a chain. */
function chainValue(chain: Chain): Value {
  return chain.source.kind === 'column' && chain.methods.length === 0
    ? { kind: 'column', column: chain.source.column }
    : { kind: 'chain', id: nextId('chain'), chain };
}

/** Returns a method that takes no argument. */
function method(name: string): Chain['methods'][number] {
  return { id: nextId('method'), name, argument: null, options: NO_OPTIONS };
}

/** Returns a method that takes `argument`. */
function methodOf(name: string, argument: Value): Chain['methods'][number] {
  return { id: nextId('method'), name, argument, options: NO_OPTIONS };
}

const column = (name: string): Value => ({ kind: 'column', column: name });

/** Sort, then added columns; later ones build on columns earlier steps add. */
export const START: readonly Step[] = [
  {
    id: 'sort',
    verb: 'sort',
    options: { by: fieldKeys(['dcn', 'timestamp']) },
  },
  {
    id: 'step_index',
    verb: 'with_columns',
    alias: 'step_index',
    value: chainValue({
      source: { kind: 'column', column: 'dcn' },
      methods: [method('cum_count'), methodOf('over', column('dcn'))],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'resolved_at',
    verb: 'with_columns',
    alias: 'resolved_at',
    value: chainValue({
      source: { kind: 'column', column: 'timestamp' },
      methods: [
        methodOf('shift', { kind: 'literal', value: 1, shown: '1' }),
        methodOf('over', column('dcn')),
      ],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'received_at',
    verb: 'with_columns',
    alias: 'received_at',
    value: chainValue({
      source: { kind: 'column', column: 'timestamp' },
      methods: [],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'resolved_day',
    verb: 'with_columns',
    alias: 'resolved_day',
    value: chainValue({
      source: { kind: 'column', column: 'resolved_at' },
      methods: [method('dt.date')],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'days_open',
    verb: 'with_columns',
    alias: 'days_open',
    value: chainValue({
      source: { kind: 'column', column: 'received_at' },
      methods: [methodOf('sub', column('resolved_at')), method('dt.total_days')],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'a_days',
    verb: 'with_columns',
    alias: 'a_days',
    value: chainValue({
      source: {
        kind: 'when',
        arms: [
          {
            id: nextId('arm'),
            condition: {
              kind: 'test',
              id: nextId('test'),
              column: 'owner',
              comparison: 'Expr.eq',
              value: { kind: 'literal', value: 'Company A', shown: 'Company A' },
            },
            returns: column('row_days'),
          },
        ],
        otherwise: { kind: 'literal', value: 0, shown: '0' },
      },
      methods: [method('cum_sum'), methodOf('over', column('dcn'))],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'join',
    verb: 'join',
    table: 'step_sla',
    options: {
      left_on: fieldKeys(['step_to']),
      right_on: fieldKeys(['step']),
      how: { kind: 'word', value: 'left' },
      maintain_order: { kind: 'word', value: 'left' },
    },
  },
  {
    id: 'sla_status',
    verb: 'with_columns',
    alias: 'sla_status',
    value: chainValue({
      source: {
        kind: 'when',
        arms: [
          {
            id: nextId('arm'),
            condition: {
              kind: 'test',
              id: nextId('test'),
              column: 'row_days',
              comparison: 'Expr.le',
              value: column('sla_days'),
            },
            returns: { kind: 'literal', value: 'within', shown: 'within' },
          },
        ],
        otherwise: { kind: 'literal', value: 'late', shown: 'late' },
      },
      methods: [],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'flag',
    verb: 'with_columns',
    alias: 'flag',
    value: chainValue({
      source: {
        kind: 'when',
        arms: [
          {
            id: nextId('arm'),
            // The day a claim was received is the day the node is run for: a chain that ends in a comparison.
            condition: {
              kind: 'chain',
              id: nextId('chain'),
              chain: {
                source: { kind: 'column', column: 'received_at' },
                methods: [method('dt.date'), methodOf('eq', { kind: 'input', name: 'as_of' })],
              },
            },
            returns: { kind: 'literal', value: 'holiday', shown: 'holiday' },
          },
        ],
        otherwise: {
          kind: 'chain',
          id: nextId('chain'),
          chain: {
            source: {
              kind: 'when',
              arms: [
                {
                  id: nextId('arm'),
                  condition: {
                    kind: 'test',
                    id: nextId('test'),
                    column: 'row_days',
                    comparison: 'Expr.gt',
                    value: { kind: 'input', name: 'sla_cutoff' },
                  },
                  returns: { kind: 'literal', value: 'over', shown: 'over' },
                },
              ],
              otherwise: { kind: 'literal', value: 'ok', shown: 'ok' },
            },
            methods: [],
          },
        },
      },
      methods: [],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'holiday_count',
    verb: 'with_columns',
    alias: 'holiday_count',
    value: chainValue({
      source: { kind: 'input', name: 'holidays' },
      methods: [method('len')],
    }),
    options: NO_OPTIONS,
  },
  {
    id: 'pivot',
    verb: 'pivot',
    on: 'step_to',
    index: 'dcn',
    values: 'row_days',
    options: {
      on_columns: { kind: 'word', value: 'step_names' },
      aggregate_function: expressionSetting({
        kind: 'chain',
        id: nextId('chain'),
        // Each cell holds one row, so the first value is the only one.
        chain: elementChain('first'),
      }),
    },
  },
];
