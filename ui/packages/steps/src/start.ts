/**
 * @file The claim-timing steps the editor opens with while it is a storyboard.
 */

import { nextId } from './model/ids';
import type { Step } from './model/steps';

/** Sort, then added columns; later ones build on columns earlier steps add. */
export const START: readonly Step[] = [
  { id: nextId('step'), verb: 'sort', by: ['dcn', 'timestamp'], descending: false },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: { kind: 'column', column: 'dcn' },
      methods: [
        { id: nextId('method'), name: 'cum_count', argument: null },
        { id: nextId('method'), name: 'over', argument: 'dcn' },
      ],
      alias: 'step_index',
    },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: { kind: 'column', column: 'timestamp' },
      methods: [
        { id: nextId('method'), name: 'shift', argument: '1' },
        { id: nextId('method'), name: 'over', argument: 'dcn' },
      ],
      alias: 'resolved_at',
    },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: { kind: 'column', column: 'timestamp' },
      methods: [],
      alias: 'received_at',
    },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: { kind: 'column', column: 'resolved_at' },
      methods: [{ id: nextId('method'), name: 'dt.date', argument: null }],
      alias: 'resolved_day',
    },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: { kind: 'column', column: 'received_at' },
      methods: [
        { id: nextId('method'), name: 'sub', argument: 'resolved_at' },
        { id: nextId('method'), name: 'dt.total_days', argument: null },
      ],
      alias: 'days_open',
    },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      source: {
        kind: 'when',
        arms: [
          {
            id: nextId('arm'),
            condition: {
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
        { id: nextId('method'), name: 'cum_sum', argument: null },
        { id: nextId('method'), name: 'over', argument: 'dcn' },
      ],
      alias: 'a_days',
    },
  },
];
