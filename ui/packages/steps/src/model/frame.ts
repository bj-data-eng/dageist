/**
 * @file Frames, columns and cells as the step editor's storyboard model sees
 * them, plus the sample frame every step list starts from.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

/** One sample value. Dates and times are kept as ISO-like text, durations as whole minutes. */
export type Cell = string | number | null;

/** The Polars dtypes the sample uses. */
export type Dtype =
  | 'String'
  | 'Int32'
  | 'Int64'
  | 'UInt32'
  | 'Datetime'
  | 'Date'
  | 'Time'
  | 'Duration';

/** One column of a frame. */
export interface Column {
  readonly name: string;
  readonly dtype: Dtype;
  readonly cells: readonly Cell[];
}

/** The rows at one point in the list. */
export type Frame = readonly Column[];

/** The frame the first step is given: seven claim rows, not yet in any useful order. */
export const SOURCE_FRAME: Frame = [
  {
    name: 'dcn',
    dtype: 'String',
    cells: ['A-1002', 'A-1001', 'A-1002', 'A-1001', 'A-1002', 'A-1001', 'A-1001'],
  },
  {
    name: 'timestamp',
    dtype: 'Datetime',
    cells: [
      '2026-03-02 13:30',
      '2026-03-05 07:50',
      '2026-03-06 10:00',
      '2026-03-02 09:15',
      '2026-03-04 17:20',
      '2026-03-09 11:05',
      '2026-03-03 16:40',
    ],
  },
  {
    name: 'step_to',
    dtype: 'String',
    cells: ['Intake', 'Pending', 'Complete', 'Intake', 'Review', 'Complete', 'Review'],
  },
  {
    name: 'owner',
    dtype: 'String',
    cells: [
      'Company A',
      'Company A',
      'Company B',
      'Company A',
      'Company A',
      'Company B',
      'Company B',
    ],
  },
  { name: 'row_days', dtype: 'Int64', cells: [0, 2, 2, 0, 2, 2, 1] },
];

/** Returns `text` as a Python string literal. */
export function quoted(text: string): string {
  return JSON.stringify(text);
}

/** Reports whether `dtype` holds numbers. */
export function isNumeric(dtype: Dtype): boolean {
  return dtype === 'Int32' || dtype === 'Int64' || dtype === 'UInt32';
}

/**
 * Returns the dtype Polars gives two numeric dtypes that meet, as in a
 * subtraction or the values of one `when`: their own when they are the same,
 * and otherwise `Int64`, the narrowest dtype that holds both.
 */
export function numericSupertype(left: Dtype, right: Dtype): Dtype {
  return left === right ? left : 'Int64';
}

/** Fails on a value the caller's `switch` was supposed to have covered. */
export function assertNever(unexpected: never): never {
  throw new Error(`Unexpected value: ${String(unexpected)}`);
}

/** Returns the column of `frame` called `name`, if there is one. */
export function findColumn(frame: Frame, name: string): Column | null {
  return frame.find((column) => column.name === name) ?? null;
}

/** Orders two cells as Polars sorts by default: nulls first, numbers by size, anything else as text. */
export function compareCells(left: Cell, right: Cell): number {
  if (left === right) return 0;
  if (left === null) return -1;
  if (right === null) return 1;
  if (typeof left === 'number' && typeof right === 'number') return left - right;
  return String(left) < String(right) ? -1 : 1;
}

/** Returns `frame` with its rows in `order`; `order` holds the old row numbers. */
export function reordered(frame: Frame, order: readonly number[]): Frame {
  return frame.map((column) => ({
    name: column.name,
    dtype: column.dtype,
    cells: order.map((row) => column.cells[row] ?? null),
  }));
}

/** Returns the distinct values of `column` in `frame`, in first-seen order. */
export function distinctValues(frame: Frame, column: string): readonly Cell[] {
  return [...new Set(findColumn(frame, column)?.cells)];
}
