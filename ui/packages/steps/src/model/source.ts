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
import {
  castCells,
  compareCells,
  findColumn,
  isNumeric,
  keysFit,
  quoted,
  supertype,
} from './frame';
import { findInput } from './inputs';

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

/** A value that comes from one of the node's connections, by its name. */
export interface InputValue {
  readonly kind: 'input';
  readonly name: string;
}

/** A value a condition compares with or returns. */
export type Value = ColumnValue | LiteralValue | ChainValue | InputValue;

/** The comparisons, as Polars and Python spell them. `is_in` tests membership in a list. */
export type Comparison = '==' | '!=' | '>' | '>=' | '<' | '<=' | 'is_in';

/** Every comparison, in the order it is offered. */
export const COMPARISONS: readonly Comparison[] = ['==', '!=', '>', '>=', '<', '<=', 'is_in'];

/** One test: `pl.col(column) <comparison> value`. */
export interface Test {
  readonly kind: 'test';
  /** Stays with the test while it is edited or moved into a group. */
  readonly id: string;
  readonly column: string;
  readonly comparison: Comparison;
  readonly value: Value;
}

/**
 * Several conditions taken together. Every one of them must hold, or any one
 * of them is enough. A group inside a group is worked out first, so nothing
 * depends on which of `&` and `|` binds tighter.
 */
export interface Group {
  readonly kind: 'group';
  /** Stays with the group while it is edited. */
  readonly id: string;
  /** Whether every item must hold. Otherwise any one of them is enough. */
  readonly every: boolean;
  readonly items: readonly Condition[];
}

/** What a filter keeps rows by, and what a `when` decides by. */
export type Condition = Test | Group;

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

/** The largest whole number an Int32 and a UInt32 hold. */
const INT32_MAX = 2_147_483_647;
const UINT32_MAX = 4_294_967_295;

/** Returns the dtype a whole number written on its own has: Int32, or Int64 when Int32 cannot hold it. */
function literalDtype(value: number): Dtype {
  return value >= -INT32_MAX - 1 && value <= INT32_MAX ? 'Int32' : 'Int64';
}

/**
 * Returns the dtype a column of integer `dtype` and the whole number `value`
 * meet in: the column's when it holds the number, and a wider one when it
 * does not. Returns null for a number past a UInt32 column's range, which
 * meets it in UInt64, a dtype this storyboard does not have.
 */
function withLiteral(dtype: Dtype, value: number): Dtype | null {
  if (dtype === 'UInt32') {
    if (value < 0) return 'Int64';
    return value > UINT32_MAX ? null : dtype;
  }
  return dtype === 'Int32' ? literalDtype(value) : dtype;
}

function checkValue(value: Value, frame: Frame): SourceCheck {
  if (value.kind === 'input') {
    const input = findInput(value.name);
    if (input === null) return { dtype: null, problem: `There is no input "${value.name}".` };
    if (input.kind !== 'value') {
      return {
        dtype: null,
        problem: `"${input.name}" is a ${input.kind}; a single value is needed here.`,
      };
    }
    return { dtype: input.dtype, problem: null };
  }
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
  return {
    dtype: typeof value.value === 'number' ? literalDtype(value.value) : 'String',
    problem: null,
  };
}

/** Returns the Polars text of `value` where a column or expression is expected. */
export function valueText(value: Value): string {
  if (value.kind === 'chain') return chainText(value.chain);
  // An input is named in the code as it is in the editor.
  if (value.kind === 'input') return value.name;
  return value.kind === 'column' ? `pl.col(${quoted(value.column)})` : literalText(value.value);
}

/** Returns the cell `value` gives on every row of `frame`. */
function cellsOf(value: Value, frame: Frame): readonly Cell[] {
  if (value.kind === 'chain') return evaluateChain(value.chain, frame) ?? [];
  if (value.kind === 'column') return findColumn(frame, value.column)?.cells ?? [];
  if (value.kind === 'input') {
    const input = findInput(value.name);
    return rowsOf(frame).map(() => (input?.kind === 'value' ? input.value : null));
  }
  return rowsOf(frame).map(() => value.value);
}

/** Reports whether two dtypes can be compared or returned side by side. */
export function dtypesAgree(left: Dtype, right: Dtype): boolean {
  return supertype(left, right) !== null;
}

/** Returns why `condition` cannot be worked out on `frame`, or `null` when it can. */
export function conditionProblem(condition: Condition, frame: Frame): string | null {
  if (condition.kind === 'group') {
    if (condition.items.length === 0) return 'A group needs at least one condition.';
    for (const item of condition.items) {
      const problem = conditionProblem(item, frame);
      if (problem !== null) return problem;
    }
    return null;
  }
  if (condition.column === '') return 'Pick the field to compare.';
  const column = findColumn(frame, condition.column);
  if (column === null) return `There is no column "${condition.column}".`;
  if (condition.comparison === 'is_in') {
    const list = condition.value.kind === 'input' ? findInput(condition.value.name) : null;
    if (list?.kind !== 'list') return 'is_in needs a list to look in.';
    return keysFit(column.dtype, list.dtype)
      ? null
      : `"${column.name}" is ${column.dtype}, but "${list.name}" holds ${list.dtype}.`;
  }
  const value = checkValue(condition.value, frame);
  if (value.problem !== null) return value.problem;
  if (value.dtype !== null && !dtypesAgree(column.dtype, value.dtype)) {
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
  if (condition.kind === 'group') {
    const items = condition.items.map((item) => conditionRows(item, frame));
    return rowsOf(frame).map((row) =>
      condition.every
        ? items.every((item) => item[row] === true)
        : items.some((item) => item[row] === true),
    );
  }
  const left = findColumn(frame, condition.column)?.cells ?? [];
  if (condition.comparison === 'is_in') {
    const list = condition.value.kind === 'input' ? findInput(condition.value.name) : null;
    const members = list?.kind === 'list' ? list.values : [];
    return rowsOf(frame).map((row) => left[row] !== null && members.includes(left[row] ?? null));
  }
  // Both sides are compared as the dtype they meet in.
  const column = findColumn(frame, condition.column)?.dtype ?? null;
  const value = checkValue(condition.value, frame).dtype;
  const met = column !== null && value !== null ? supertype(column, value) : null;
  const compared = castCells(left, column, met);
  const right = castCells(cellsOf(condition.value, frame), value, met);
  return rowsOf(frame).map((row) =>
    holds(condition.comparison, compared[row] ?? null, right[row] ?? null),
  );
}

/** Returns the Polars text of `condition`. */
export function conditionText(condition: Condition): string {
  if (condition.kind === 'group') {
    // Every item stands in brackets, so the text says what the lists say.
    const joined = condition.items.map((item) => `(${conditionText(item)})`);
    return joined.join(condition.every ? ' & ' : ' | ');
  }
  if (condition.comparison === 'is_in') {
    return `pl.col(${quoted(condition.column)}).is_in(${valueText(condition.value)})`;
  }
  return `pl.col(${quoted(condition.column)}) ${condition.comparison} ${valueText(condition.value)}`;
}

/** Returns the Polars text of a value passed to `then` or `otherwise`. */
function returnedText(value: Value): string {
  // A bare string there would be read as a column name.
  return value.kind === 'literal' && typeof value.value === 'string'
    ? `pl.lit(${quoted(value.value)})`
    : valueText(value);
}

const STRING: SourceCheck = { dtype: 'String', problem: null };

function differing(first: Dtype, second: Dtype): SourceCheck {
  return {
    dtype: null,
    problem: `then and otherwise return different dtypes: ${first} and ${second}.`,
  };
}

function checkWhen(source: WhenSource, frame: Frame): SourceCheck {
  for (const arm of source.arms) {
    const problem = conditionProblem(arm.condition, frame);
    if (problem !== null) return { dtype: null, problem };
  }
  // A literal has no dtype of its own beside a column: it takes the column's,
  // widened when it does not fit. So literals wait until the dtypes of the
  // columns, subchains and inputs have met.
  let typed: Dtype | null = null;
  const numbers: number[] = [];
  let text = false;
  for (const value of [...source.arms.map((arm) => arm.returns), source.otherwise]) {
    if (value.kind === 'literal') {
      if (typeof value.value === 'number') numbers.push(value.value);
      else if (value.value !== null) text = true;
      continue;
    }
    const checked = checkValue(value, frame);
    if (checked.problem !== null) return checked;
    if (checked.dtype === null) continue;
    if (typed === null) {
      typed = checked.dtype;
      continue;
    }
    const met = supertype(typed, checked.dtype);
    if (met === null) return differing(typed, checked.dtype);
    typed = met;
  }
  const widest: Dtype = numbers.some((number) => literalDtype(number) === 'Int64')
    ? 'Int64'
    : 'Int32';
  if (typed === null) {
    if (text) return numbers.length > 0 ? differing('String', widest) : STRING;
    return numbers.length > 0
      ? { dtype: widest, problem: null }
      : { dtype: null, problem: 'when needs at least one value that is not None.' };
  }
  if (text && typed !== 'String') return differing(typed, 'String');
  let dtype = typed;
  for (const number of numbers) {
    if (!isNumeric(dtype)) return differing(dtype, widest);
    const met = withLiteral(dtype, number);
    if (met === null) {
      return {
        dtype: null,
        problem: `${String(number)} beside ${dtype} is UInt64, which this storyboard does not show.`,
      };
    }
    dtype = met;
  }
  return { dtype, problem: null };
}

/** Returns the dtype `source` gives on `frame`, or why it gives none. */
export function checkSource(source: Source, frame: Frame): SourceCheck {
  if (source.kind === 'when') return checkWhen(source, frame);
  if (source.column === '') {
    return { dtype: null, problem: 'Pick the field the chain starts from.' };
  }
  const column = findColumn(frame, source.column);
  return column === null
    ? { dtype: null, problem: `There is no column "${source.column}".` }
    : { dtype: column.dtype, problem: null };
}

/** Works `source` out on the rows of `frame`. */
function evaluateSource(source: Source, frame: Frame): readonly Cell[] {
  if (source.kind === 'column') return findColumn(frame, source.column)?.cells ?? [];
  // Every value is returned as the dtype they all meet in.
  const result = checkWhen(source, frame).dtype;
  const returned = (value: Value): readonly Cell[] =>
    castCells(cellsOf(value, frame), checkValue(value, frame).dtype, result);
  const arms = source.arms.map((arm) => ({
    holds: conditionRows(arm.condition, frame),
    cells: returned(arm.returns),
  }));
  const otherwise = returned(source.otherwise);
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
    const operand = findColumn(frame, method.argument ?? '')?.dtype ?? null;
    const next = methodDtype(method.name, current, operand);
    if (next === null) {
      const message = `${method.name} needs ${methodRequirement(method.name)}, but it is given ${current}.`;
      return { dtypes, problem: { index, message } };
    }
    const wrongArgument = argumentProblem(method, frame, current);
    // The dtype fits even when the argument is wrong, so it is still reported.
    dtypes.push(next);
    if (wrongArgument !== null) return { dtypes, problem: { index, message: wrongArgument } };
    current = next;
  }
  return { dtypes, problem: null };
}

/** The number of values a UInt32 holds: an unsigned subtraction below 0 wraps round by it. */
const UINT32_SPAN = 4_294_967_296;

/** Works `chain` out on the rows of `frame`, or returns null when it has a problem. */
export function evaluateChain(chain: Chain, frame: Frame): readonly Cell[] | null {
  const check = checkChain(chain, frame);
  if (check.problem !== null) return null;
  let values = evaluateSource(chain.source, frame);
  const rowCount = frame[0]?.cells.length ?? 0;
  const apply = (index: number, groups: readonly number[][]): void => {
    const method = chain.methods[index];
    if (method === undefined) return;
    values = applyWithin(method, values, groups, frame);
    if (method.name === 'sub' && check.dtypes[index + 1] === 'UInt32') {
      // Polars subtracts unsigned integers without widening them.
      values = values.map((value) =>
        typeof value === 'number' && value < 0 ? value + UINT32_SPAN : value,
      );
    }
  };
  // The methods before an `over` are worked out within its groups.
  let waiting: number[] = [];
  for (const [index, method] of chain.methods.entries()) {
    if (method.name === 'over') {
      const keys = findColumn(frame, method.argument ?? '')?.cells ?? null;
      const groups = rowGroups(keys, rowCount);
      for (const earlier of waiting) apply(earlier, groups);
      waiting = [];
    } else {
      waiting.push(index);
    }
  }
  const wholeFrame = rowGroups(null, rowCount);
  for (const earlier of waiting) apply(earlier, wholeFrame);
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
