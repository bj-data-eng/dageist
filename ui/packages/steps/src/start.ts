/**
 * @file The claim-timing steps the editor opens with while it is a storyboard.
 */

import { nextId } from './model/ids';
import type { Step } from './model/steps';

/** Sort, then four added columns; the last one builds on an earlier one. */
export const START: readonly Step[] = [
  { id: nextId('step'), verb: 'sort', by: ['dcn', 'timestamp'], descending: false },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      column: 'dcn',
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
      column: 'timestamp',
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
    expr: { column: 'timestamp', methods: [], alias: 'received_at' },
  },
  {
    id: nextId('step'),
    verb: 'with_columns',
    expr: {
      column: 'resolved_at',
      methods: [{ id: nextId('method'), name: 'dt.date', argument: null }],
      alias: 'resolved_day',
    },
  },
];
