/**
 * @file Chains and what they are made of. A chain starts from a source, a
 * column or a condition that picks between values (`when … then …
 * otherwise`), and applies methods to it. A value inside a condition can
 * itself be a chain, so chains nest.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Method } from './expr';
import {
  applyWithin,
  argumentProblem,
  methodDtype,
  methodRequirement,
  methodText,
  rowGroups,
} from './expr';
import type { Cell, Dtype, Frame } from './frame';
import { compareCells, findColumn, isNumeric, quoted } from './frame';

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

/** A value worked out by a chain of its own. */
export interface ChainValue {
  readonly kind: 'chain';
  /** Stays with the sub-chain while it is edited. */
  readonly id: string;
  readonly chain: Chain;
}

/** A value a condition compares with or returns. */
export type Value = ColumnValue | LiteralValue | ChainValue;

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

/** A chain that starts from `pl.col(column)`. */
export interface ColumnSource {
  readonly kind: 'column';
  readonly column: string;
}

/** A chain that starts from `pl.when(…).then(…)` pairs and a final `.otherwise(…)`. */
export interface WhenSource {
  readonly kind: 'when';
  readonly arms: readonly Arm[];
  readonly otherwise: Value;
}

/** What a chain starts from. */
export type Source = ColumnSource | WhenSource;

/** A source followed by method calls. */
export interface Chain {
  readonly source: Source;
  readonly methods: readonly Method[];
}

/** A chain with a final `.alias(alias)`: what an Add column step holds. */
export interface Expr extends Chain {
  readonly alias: string;
}

/** Why a chain cannot be worked out, and at which method. Index -1 is the source. */
export interface ChainProblem {
  readonly index: number;
  readonly message: string;
}

/** The dtype after the source and after each method, plus the first problem. */
export interface ChainCheck {
  readonly dtypes: readonly Dtype[];
  readonly problem: ChainProblem | null;
}

/** The dtype a source or value has, or why it has none. */
export interface SourceCheck {
  /** `null` for a literal None, which takes the dtype of what it is used with. */
  readonly dtype: Dtype | null;
  readonly problem: string | null;
}

function rowsOf(frame: Frame): number[] {
  return Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
}

function checkValue(value: Value, frame: Frame): SourceCheck {
  if (value.kind === 'chain') {
    const check = checkChain(value.chain, frame);
    return check.problem === null
      ? { dtype: check.dtypes.at(-1) ?? null, problem: null }
      : { dtype: null, problem: check.problem.message };
  }
  if (value.kind === 'column') {
    const column = findColumn(frame, value.column);
    return column === null
      ? { dtype: null, problem: `There is no column "${value.column}".` }
      : { dtype: column.dtype, problem: null };
  }
  if (value.value === null) return { dtype: null, problem: null };
  return { dtype: typeof value.value === 'number' ? 'Int64' : 'String', problem: null };
}

/** Returns the Polars text of `value` where a column or expression is expected. */
export function valueText(value: Value): string {
  if (value.kind === 'chain') return chainText(value.chain);
  return value.kind === 'column' ? `pl.col(${quoted(value.column)})` : literalText(value.value);
}

/** Returns the cell `value` gives on every row of `frame`. */
function cellsOf(value: Value, frame: Frame): readonly Cell[] {
  if (value.kind === 'chain') return evaluateChain(value.chain, frame) ?? [];
  if (value.kind === 'column') return findColumn(frame, value.column)?.cells ?? [];
  return rowsOf(frame).map(() => value.value);
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
    return `"${column.name}" is ${column.dtype}, but ${valueText(condition.value)} is ${value.dtype}.`;
  }
  return null;
}

function holds(comparison: Comparison, left: Cell, right: Cell): boolean {
  // A comparison with a null is not true.
  if (left === null || right === null) return false;
  const order = compareCells(left, right);
  switch (comparison) {
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

/** Reports, for every row of `frame`, whether `condition` is true on it. */
export function conditionRows(condition: Condition, frame: Frame): boolean[] {
  const left = findColumn(frame, condition.column)?.cells ?? [];
  const right = cellsOf(condition.value, frame);
  return rowsOf(frame).map((row) =>
    holds(condition.comparison, left[row] ?? null, right[row] ?? null),
  );
}

/** Returns the Polars text of `condition`. */
export function conditionText(condition: Condition): string {
  return `pl.col(${quoted(condition.column)}) ${condition.comparison} ${valueText(condition.value)}`;
}

/** Returns the Polars text of a value passed to `then` or `otherwise`. */
function returnedText(value: Value): string {
  // A bare string there would be read as a column name.
  return value.kind === 'literal' && typeof value.value === 'string'
    ? `pl.lit(${quoted(value.value)})`
    : valueText(value);
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
function evaluateSource(source: Source, frame: Frame): readonly Cell[] {
  if (source.kind === 'column') return findColumn(frame, source.column)?.cells ?? [];
  const arms = source.arms.map((arm) => ({
    holds: conditionRows(arm.condition, frame),
    cells: cellsOf(arm.returns, frame),
  }));
  const otherwise = cellsOf(source.otherwise, frame);
  return rowsOf(frame).map((row) => {
    const arm = arms.find((candidate) => candidate.holds[row] === true);
    return (arm === undefined ? otherwise[row] : arm.cells[row]) ?? null;
  });
}

/** Returns the Polars text `source` starts a chain with. */
function sourceText(source: Source): string {
  if (source.kind === 'column') return `pl.col(${quoted(source.column)})`;
  const arms = source.arms
    .map((arm) => `.when(${conditionText(arm.condition)}).then(${returnedText(arm.returns)})`)
    .join('');
  return `pl${arms}.otherwise(${returnedText(source.otherwise)})`;
}

/** Checks that the source of `chain` works and every method accepts the dtype handed to it. */
export function checkChain(chain: Chain, frame: Frame): ChainCheck {
  const start = checkSource(chain.source, frame);
  if (start.problem !== null || start.dtype === null) {
    const message = start.problem ?? 'This chain has no dtype.';
    return { dtypes: [], problem: { index: -1, message } };
  }
  const dtypes: Dtype[] = [start.dtype];
  let current = start.dtype;
  for (const [index, method] of chain.methods.entries()) {
    const next = methodDtype(method.name, current);
    if (next === null) {
      const message = `${method.name} needs ${methodRequirement(method.name)}, but it is given ${current}.`;
      return { dtypes, problem: { index, message } };
    }
    const wrongArgument = argumentProblem(method, frame, current);
    if (wrongArgument !== null) return { dtypes, problem: { index, message: wrongArgument } };
    dtypes.push(next);
    current = next;
  }
  return { dtypes, problem: null };
}

/** Works `chain` out on the rows of `frame`, or returns null when it has a problem. */
export function evaluateChain(chain: Chain, frame: Frame): readonly Cell[] | null {
  if (checkChain(chain, frame).problem !== null) return null;
  let values = evaluateSource(chain.source, frame);
  let waiting: Method[] = [];
  const rowCount = frame[0]?.cells.length ?? 0;
  for (const method of chain.methods) {
    if (method.name === 'over') {
      const keys = findColumn(frame, method.argument ?? '')?.cells ?? null;
      const groups = rowGroups(keys, rowCount);
      for (const earlier of waiting) {
        values = applyWithin(earlier, values, groups, frame);
      }
      waiting = [];
    } else {
      waiting.push(method);
    }
  }
  const wholeFrame = rowGroups(null, rowCount);
  for (const earlier of waiting) {
    values = applyWithin(earlier, values, wholeFrame, frame);
  }
  return values;
}

/** Returns the Polars text of `chain`. */
export function chainText(chain: Chain): string {
  return `${sourceText(chain.source)}${chain.methods.map((method) => methodText(method)).join('')}`;
}

/** Returns the Polars text of `expr`: its chain and its alias. */
export function exprText(expr: Expr): string {
  return `${chainText(expr)}.alias(${quoted(expr.alias)})`;
}
