/**
 * @file What the step editor's node is connected to, while the editor is a
 * storyboard. In the real application these come from the node graph: every
 * connection into the node is an input, and nothing else is in scope. An
 * input is a table, a single value or a list, and each has its dtypes.
 */

import type { Cell, Dtype, Frame } from './frame';
import { SOURCE_FRAME } from './frame';

/** A connected table: another node's rows. */
export interface TableInput {
  readonly kind: 'table';
  readonly name: string;
  /** The node the connection comes from. */
  readonly from: string;
  readonly frame: Frame;
  /** Whether the steps start from this table's rows. */
  readonly primary: boolean;
}

/** A connected single value, such as a cutoff or an as-of date. */
export interface ValueInput {
  readonly kind: 'value';
  readonly name: string;
  readonly from: string;
  readonly dtype: Dtype;
  readonly value: Cell;
}

/** A connected list of values of one dtype, such as a column of another table. */
export interface ListInput {
  readonly kind: 'list';
  readonly name: string;
  readonly from: string;
  readonly dtype: Dtype;
  readonly values: readonly Cell[];
}

/** One connection into the node. */
export type Input = TableInput | ValueInput | ListInput;

/** The connections the sample node has. */
export const INPUTS: readonly Input[] = [
  { kind: 'table', name: 'claims', from: 'Claims · Parquet', frame: SOURCE_FRAME, primary: true },
  {
    kind: 'table',
    name: 'step_sla',
    from: 'SLA by step · CSV',
    primary: false,
    frame: [
      { name: 'step', dtype: 'String', cells: ['Intake', 'Review', 'Pending', 'Complete'] },
      { name: 'sla_days', dtype: 'Int64', cells: [1, 1, 2, 3] },
    ],
  },
  {
    kind: 'table',
    name: 'owner_teams',
    from: 'Owners · Parquet',
    primary: false,
    frame: [
      { name: 'owner', dtype: 'String', cells: ['Company A', 'Company B'] },
      { name: 'team', dtype: 'String', cells: ['Claims North', 'Claims South'] },
      { name: 'owner_id', dtype: 'Int64', cells: [101, 102] },
    ],
  },
  { kind: 'value', name: 'sla_cutoff', from: 'Parameters', dtype: 'Int64', value: 1 },
  { kind: 'value', name: 'as_of', from: 'Parameters', dtype: 'Date', value: '2026-03-10' },
  {
    kind: 'list',
    name: 'holidays',
    from: 'Holiday calendar · column "date"',
    dtype: 'Date',
    values: ['2026-03-03', '2026-03-05'],
  },
];

/** Returns the input called `name`, if the node has one. */
export function findInput(name: string): Input | null {
  return INPUTS.find((input) => input.name === name) ?? null;
}
