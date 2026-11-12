/**
 * @file What an expression can start from: a column, or a condition that
 * picks between values (`when … then … otherwise`). Also the values and
 * conditions those are made of, which a filter uses too.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Cell, Dtype, Frame } from './frame';
import { compareCells, findColumn, isNumeric } from './frame';

/** Returns `text` as a Python string literal. */
export function quoted(text: string): string {
  return JSON.stringify(text);
}

/** Returns `value` as Python would write it. */
export function literalText(value: Cell): string {
  if (value === null) return 'None';
  return typeof value === 'number' ? String(value) : quoted(value);
}

/** A value taken from a column, row by row. */
export interface ColumnValue {
  readonly kind: 'column';
  readonly column: string;
}

/** The same value on every row. */
export interface LiteralValue {
  readonly kind: 'literal';
  readonly value: Cell;
}

/** A value a condition compares with or returns. */
export type Value = ColumnValue | LiteralValue;

/** The comparison operators, as Polars and Python spell them. */
export type Comparison = '==' | '!=' | '>' | '>=' | '<' | '<=';

/** Every comparison, in the order it is offered. */
export const COMPARISONS: readonly Comparison[] = ['==', '!=', '>', '>=', '<', '<='];

/** `pl.col(column) <comparison> value`. */
export interface Condition {
  readonly column: string;
  readonly comparison: Comparison;
  readonly value: Value;
}

/** One `.when(condition).then(value)` pair. */
export interface Arm {
  /** Stays with the arm while it is edited. */
  readonly id: string;
  readonly condition: Condition;
  readonly returns: Value;
}

/** An expression that starts from `pl.col(column)`. */
export interface ColumnSource {
  readonly kind: 'column';
  readonly column: string;
}

/** An expression that starts from `pl.when(…).then(…)` pairs and a final `.otherwise(…)`. */
export interface WhenSource {
  readonly kind: 'when';
  readonly arms: readonly Arm[];
  readonly otherwise: Value;
}

/** What an expression starts from. */
export type Source = ColumnSource | WhenSource;

/** The dtype a source or value has, or why it has none. */
export interface SourceCheck {
  /** `null` for a literal None, which takes the dtype of what it is used with. */
  readonly dtype: Dtype | null;
  readonly problem: string | null;
}

function checkValue(value: Value, frame: Frame): SourceCheck {
  if (value.kind === 'column') {
    const column = findColumn(frame, value.column);
    return column === null
      ? { dtype: null, problem: `There is no column "${value.column}".` }
      : { dtype: column.dtype, problem: null };
  }
  if (value.value === null) return { dtype: null, problem: null };
  return { dtype: typeof value.value === 'number' ? 'Int64' : 'String', problem: null };
}

function describeValue(value: Value): string {
  return value.kind === 'column' ? `"${value.column}"` : literalText(value.value);
}

/** Reports whether two dtypes can be compared or returned side by side. */
function agree(left: Dtype, right: Dtype): boolean {
  return left === right || (isNumeric(left) && isNumeric(right));
}

/** Returns why `condition` cannot be worked out on `frame`, or `null` when it can. */
export function conditionProblem(condition: Condition, frame: Frame): string | null {
  const column = findColumn(frame, condition.column);
  if (column === null) return `There is no column "${condition.column}".`;
  const value = checkValue(condition.value, frame);
  if (value.problem !== null) return value.problem;
  if (value.dtype !== null && !agree(column.dtype, value.dtype)) {
    return `"${column.name}" is ${column.dtype}, but ${describeValue(condition.value)} is ${value.dtype}.`;
  }
  return null;
}

function cellOf(value: Value, frame: Frame, row: number): Cell {
  if (value.kind === 'literal') return value.value;
  return findColumn(frame, value.column)?.cells[row] ?? null;
}

/** Reports whether `condition` is true on `row`. A comparison with a null is not true. */
export function conditionHolds(condition: Condition, frame: Frame, row: number): boolean {
  const left = findColumn(frame, condition.column)?.cells[row] ?? null;
  const right = cellOf(condition.value, frame, row);
  if (left === null || right === null) return false;
  const order = compareCells(left, right);
  switch (condition.comparison) {
    case '==':
      return order === 0;
    case '!=':
      return order !== 0;
    case '>':
      return order > 0;
    case '>=':
      return order >= 0;
    case '<':
      return order < 0;
    case '<=':
      return order <= 0;
    default:
      return false;
  }
}

/** Returns the Polars text of `condition`. */
export function conditionText(condition: Condition): string {
  const value =
    condition.value.kind === 'column'
      ? `pl.col(${quoted(condition.value.column)})`
      : literalText(condition.value.value);
  return `pl.col(${quoted(condition.column)}) ${condition.comparison} ${value}`;
}

/** Returns the Polars text of a value passed to `then` or `otherwise`. */
function returnedText(value: Value): string {
  if (value.kind === 'column') return `pl.col(${quoted(value.column)})`;
  // A bare string there would be read as a column name.
  return typeof value.value === 'string'
    ? `pl.lit(${quoted(value.value)})`
    : literalText(value.value);
}

function checkWhen(source: WhenSource, frame: Frame): SourceCheck {
  const dtypes: Dtype[] = [];
  for (const arm of source.arms) {
    const problem = conditionProblem(arm.condition, frame);
    if (problem !== null) return { dtype: null, problem };
  }
  for (const value of [...source.arms.map((arm) => arm.returns), source.otherwise]) {
    const checked = checkValue(value, frame);
    if (checked.problem !== null) return checked;
    if (checked.dtype !== null) dtypes.push(checked.dtype);
  }
  const [first] = dtypes;
  if (first === undefined) {
    return { dtype: null, problem: 'when needs at least one value that is not None.' };
  }
  const odd = dtypes.find((dtype) => !agree(first, dtype));
  if (odd !== undefined) {
    return {
      dtype: null,
      problem: `then and otherwise return different dtypes: ${first} and ${odd}.`,
    };
  }
  return { dtype: dtypes.every((dtype) => dtype === 'UInt32') ? 'UInt32' : first, problem: null };
}

/** Returns the dtype `source` gives on `frame`, or why it gives none. */
export function checkSource(source: Source, frame: Frame): SourceCheck {
  if (source.kind === 'when') return checkWhen(source, frame);
  const column = findColumn(frame, source.column);
  return column === null
    ? { dtype: null, problem: `There is no column "${source.column}".` }
    : { dtype: column.dtype, problem: null };
}

/** Works `source` out on the rows of `frame`. */
export function evaluateSource(source: Source, frame: Frame): readonly Cell[] {
  if (source.kind === 'column') return findColumn(frame, source.column)?.cells ?? [];
  const rows = Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
  return rows.map((row) => {
    const arm = source.arms.find((candidate) => conditionHolds(candidate.condition, frame, row));
    return cellOf(arm === undefined ? source.otherwise : arm.returns, frame, row);
  });
}

/** Returns the Polars text `source` starts an expression with. */
export function sourceText(source: Source): string {
  if (source.kind === 'column') return `pl.col(${quoted(source.column)})`;
  const arms = source.arms
    .map((arm) => `.when(${conditionText(arm.condition)}).then(${returnedText(arm.returns)})`)
    .join('');
  return `pl${arms}.otherwise(${returnedText(source.otherwise)})`;
}
