/**
 * @file The claim-timing steps the editor opens with while it is a storyboard.
 */

import { nextId } from './model/ids';
import { NO_OPTIONS } from './model/settings';
import type { Chain, Value } from './model/source';
import type { Step } from './model/steps';

/** Returns `chain` as what a column holds: a field alone is that field, anything more is a chain. */
function chainValue(chain: Chain): Value {
  return chain.source.kind === 'column' && chain.methods.length === 0
    ? { kind: 'column', column: chain.source.column }
    : { kind: 'chain', id: nextId('chain'), chain };
}

/** Sort, then added columns; later ones build on columns earlier steps add. */
export const START: readonly Step[] = [
  {
    id: nextId('step'),
    verb: 'sort',
    by: ['dcn', 'timestamp'],
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'step_index',
    value: chainValue({
      source: { kind: 'column', column: 'dcn' },
      methods: [
        { id: nextId('method'), name: 'cum_count', argument: null, options: NO_OPTIONS },
        { id: nextId('method'), name: 'over', argument: 'dcn', options: NO_OPTIONS },
      ],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'resolved_at',
    value: chainValue({
      source: { kind: 'column', column: 'timestamp' },
      methods: [
        { id: nextId('method'), name: 'shift', argument: '1', options: NO_OPTIONS },
        { id: nextId('method'), name: 'over', argument: 'dcn', options: NO_OPTIONS },
      ],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'received_at',
    value: chainValue({
      source: { kind: 'column', column: 'timestamp' },
      methods: [],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'resolved_day',
    value: chainValue({
      source: { kind: 'column', column: 'resolved_at' },
      methods: [{ id: nextId('method'), name: 'dt.date', argument: null, options: NO_OPTIONS }],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'days_open',
    value: chainValue({
      source: { kind: 'column', column: 'received_at' },
      methods: [
        { id: nextId('method'), name: 'sub', argument: 'resolved_at', options: NO_OPTIONS },
        { id: nextId('method'), name: 'dt.total_days', argument: null, options: NO_OPTIONS },
      ],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
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
              comparison: '==',
              value: { kind: 'literal', value: 'Company A' },
            },
            returns: { kind: 'column', column: 'row_days' },
          },
        ],
        otherwise: { kind: 'literal', value: 0 },
      },
      methods: [
        { id: nextId('method'), name: 'cum_sum', argument: null, options: NO_OPTIONS },
        { id: nextId('method'), name: 'over', argument: 'dcn', options: NO_OPTIONS },
      ],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'join',
    table: 'step_sla',
    leftOn: ['step_to'],
    rightOn: ['step'],
    options: { how: { kind: 'word', value: 'left' } },
  },
  {
    id: nextId('step'),
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
              comparison: '<=',
              value: { kind: 'column', column: 'sla_days' },
            },
            returns: { kind: 'literal', value: 'within' },
          },
        ],
        otherwise: { kind: 'literal', value: 'late' },
      },
      methods: [],
    }),
    options: NO_OPTIONS,
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    alias: 'flag',
    value: chainValue({
      source: {
        kind: 'when',
        arms: [
          {
            id: nextId('arm'),
            condition: {
              kind: 'test',
              id: nextId('test'),
              column: 'resolved_day',
              comparison: 'is_in',
              value: { kind: 'input', name: 'holidays' },
            },
            returns: { kind: 'literal', value: 'holiday' },
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
                    comparison: '>',
                    value: { kind: 'input', name: 'sla_cutoff' },
                  },
                  returns: { kind: 'literal', value: 'over' },
                },
              ],
              otherwise: { kind: 'literal', value: 'ok' },
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
    id: nextId('step'),
    verb: 'pivot',
    on: 'step_to',
    index: 'dcn',
    values: 'row_days',
    aggregate: 'sum',
    columnsFrom: 'collect',
    pinned: [],
    options: NO_OPTIONS,
  },
];
