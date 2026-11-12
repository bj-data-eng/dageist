/**
 * @file Expressions in Polars' own vocabulary: `pl.col(…)` followed by method
 * calls and a final alias. Knows which dtype each method accepts and returns,
 * and works an expression out on a frame.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Cell, Dtype, Frame } from './frame';
import { assertNever, compareCells, findColumn } from './frame';

/** Every expression method the prototype knows, by its Polars name. */
export type MethodName =
  | 'shift'
  | 'first'
  | 'last'
  | 'cum_sum'
  | 'cum_count'
  | 'cum_max'
  | 'dt.date'
  | 'dt.time'
  | 'dt.offset_by'
  | 'dt.month_start'
  | 'over';

/** Every method, in the order it is offered. */
export const METHOD_NAMES: readonly MethodName[] = [
  'shift',
  'first',
  'last',
  'cum_sum',
  'cum_count',
  'cum_max',
  'dt.date',
  'dt.time',
  'dt.offset_by',
  'dt.month_start',
  'over',
];

/** A group of methods that can stand in for one another. */
export type MethodGroup = 'rows' | 'cumulative' | 'dt' | 'window';

/** Every group, in the order it is offered. */
export const METHOD_GROUP_ORDER: readonly MethodGroup[] = ['rows', 'cumulative', 'dt', 'window'];

/** Headings for the groups. */
export const METHOD_GROUP_TITLES: Readonly<Record<MethodGroup, string>> = {
  rows: 'Rows',
  cumulative: 'Cumulative',
  dt: 'dt · dates and times',
  window: 'Window',
};

/** The group each method belongs to. */
export const METHOD_GROUPS: Readonly<Record<MethodName, MethodGroup>> = {
  shift: 'rows',
  first: 'rows',
  last: 'rows',
  cum_sum: 'cumulative',
  cum_count: 'cumulative',
  cum_max: 'cumulative',
  'dt.date': 'dt',
  'dt.time': 'dt',
  'dt.offset_by': 'dt',
  'dt.month_start': 'dt',
  over: 'window',
};

/** What each method does, for someone who does not know the name yet. */
export const METHOD_HINTS: Readonly<Record<MethodName, string>> = {
  shift: 'value n rows earlier',
  first: 'first value',
  last: 'last value',
  cum_sum: 'running total',
  cum_count: 'running count',
  cum_max: 'running maximum',
  'dt.date': 'the date part',
  'dt.time': 'the time part',
  'dt.offset_by': 'move by a duration',
  'dt.month_start': 'first of the month',
  over: 'within each group',
};

/** What a method's one argument is, or `null` when it takes none. */
export type ArgumentKind = 'integer' | 'duration' | 'column';

/** The kind of argument each method takes. */
export const ARGUMENT_KINDS: Readonly<Record<MethodName, ArgumentKind | null>> = {
  shift: 'integer',
  first: null,
  last: null,
  cum_sum: null,
  cum_count: null,
  cum_max: null,
  'dt.date': null,
  'dt.time': null,
  'dt.offset_by': 'duration',
  'dt.month_start': null,
  over: 'column',
};

/** One method call. `argument` holds the argument as typed, or `null`. */
export interface Method {
  /** Stays with the call while it is moved or edited. */
  readonly id: string;
  readonly name: MethodName;
  readonly argument: string | null;
}

/** `pl.col(column)` followed by method calls and a final `.alias(alias)`. */
export interface Expr {
  readonly column: string;
  readonly methods: readonly Method[];
  readonly alias: string;
}

/** Why an expression cannot be worked out, and at which method. Index -1 is `pl.col`. */
export interface ExprProblem {
  readonly index: number;
  readonly message: string;
}

/** The dtype after `pl.col` and after each method, plus the first problem. */
export interface ExprCheck {
  readonly dtypes: readonly Dtype[];
  readonly problem: ExprProblem | null;
}

const DURATION = /^(?<amount>-?\d+)(?<unit>d|w|mo)$/;
const INTEGER = /^-?\d+$/;

function isNumeric(dtype: Dtype): boolean {
  return dtype === 'Int64' || dtype === 'UInt32';
}

function isTemporal(dtype: Dtype): boolean {
  return dtype === 'Datetime' || dtype === 'Date' || dtype === 'Time';
}

/** Returns the dtype `name` produces from `input`, or null when it does not accept it. */
export function methodDtype(name: MethodName, input: Dtype): Dtype | null {
  switch (name) {
    case 'shift':
    case 'first':
    case 'last':
    case 'over':
      return input;
    case 'cum_count':
      return 'UInt32';
    case 'cum_sum':
      // Polars widens only integers narrower than 32 bits before summing.
      return isNumeric(input) ? input : null;
    case 'cum_max':
      return isNumeric(input) || isTemporal(input) ? input : null;
    case 'dt.date':
      return input === 'Datetime' ? 'Date' : null;
    case 'dt.time':
      return input === 'Datetime' ? 'Time' : null;
    case 'dt.offset_by':
    case 'dt.month_start':
      return input === 'Datetime' || input === 'Date' ? input : null;
    default:
      return assertNever(name);
  }
}

/** Returns the dtypes `name` accepts, such as "Datetime or Date". */
export function methodRequirement(name: MethodName): string {
  switch (name) {
    case 'shift':
    case 'first':
    case 'last':
    case 'over':
    case 'cum_count':
      return 'any dtype';
    case 'cum_sum':
      return 'a numeric dtype';
    case 'cum_max':
      return 'a numeric or temporal dtype';
    case 'dt.date':
    case 'dt.time':
      return 'Datetime';
    case 'dt.offset_by':
    case 'dt.month_start':
      return 'Datetime or Date';
    default:
      return assertNever(name);
  }
}

function argumentProblem(method: Method, frame: Frame): string | null {
  const kind = ARGUMENT_KINDS[method.name];
  const argument = method.argument ?? '';
  switch (kind) {
    case null:
      return null;
    case 'integer':
      return INTEGER.test(argument) ? null : `${method.name} takes a whole number of rows.`;
    case 'duration':
      return DURATION.test(argument)
        ? null
        : `${method.name} takes a duration such as "-1d", "2w" or "1mo".`;
    case 'column':
      return findColumn(frame, argument) === null ? `There is no column "${argument}".` : null;
    default:
      return assertNever(kind);
  }
}

/** Checks that every method of `expr` accepts the dtype handed to it. */
export function checkExpr(expr: Expr, frame: Frame): ExprCheck {
  const column = findColumn(frame, expr.column);
  if (column === null) {
    const message = `There is no column "${expr.column}".`;
    return { dtypes: [], problem: { index: -1, message } };
  }
  const dtypes: Dtype[] = [column.dtype];
  let current = column.dtype;
  for (const [index, method] of expr.methods.entries()) {
    const next = methodDtype(method.name, current);
    if (next === null) {
      const message = `${method.name} needs ${methodRequirement(method.name)}, but it is given ${current}.`;
      return { dtypes, problem: { index, message } };
    }
    const wrongArgument = argumentProblem(method, frame);
    if (wrongArgument !== null) return { dtypes, problem: { index, message: wrongArgument } };
    dtypes.push(next);
    current = next;
  }
  return { dtypes, problem: null };
}

function offsetBy(value: string, duration: string): string {
  const groups = DURATION.exec(duration)?.groups;
  const amount = Number(groups?.amount ?? 0);
  const moment = new Date(`${value.slice(0, 10)}T00:00:00Z`);
  if (groups?.unit === 'mo') {
    moment.setUTCMonth(moment.getUTCMonth() + amount);
  } else {
    moment.setUTCDate(moment.getUTCDate() + amount * (groups?.unit === 'w' ? 7 : 1));
  }
  return `${moment.toISOString().slice(0, 10)}${value.slice(10)}`;
}

function applyToEach(method: Method, value: Cell): Cell {
  if (typeof value !== 'string') return value;
  switch (method.name) {
    case 'dt.date':
      return value.slice(0, 10);
    case 'dt.time':
      return value.slice(11, 16);
    case 'dt.offset_by':
      return offsetBy(value, method.argument ?? '');
    case 'dt.month_start':
      return `${value.slice(0, 7)}-01${value.slice(10)}`;
    default:
      return value;
  }
}

function runningMaximum(values: readonly Cell[]): Cell[] {
  let highest: Cell = null;
  return values.map((value) => {
    if (value !== null && (highest === null || compareCells(value, highest) > 0)) highest = value;
    return highest;
  });
}

function applyAcrossGroup(method: Method, values: readonly Cell[]): Cell[] {
  switch (method.name) {
    case 'shift': {
      const rows = Number(method.argument ?? 0);
      return values.map((_value, position) => values[position - rows] ?? null);
    }
    case 'first':
      return values.map(() => values[0] ?? null);
    case 'last':
      return values.map(() => values.at(-1) ?? null);
    case 'cum_count': {
      let count = 0;
      return values.map((value) => {
        if (value !== null) count += 1;
        return count;
      });
    }
    case 'cum_sum': {
      let total = 0;
      return values.map((value) => {
        if (typeof value === 'number') total += value;
        return total;
      });
    }
    case 'cum_max':
      return runningMaximum(values);
    default:
      return values.map((value) => applyToEach(method, value));
  }
}

function rowGroups(keys: readonly Cell[] | null, rowCount: number): number[][] {
  const everyRow = Array.from({ length: rowCount }, (_unused, index) => index);
  if (keys === null) return [everyRow];
  const groups = new Map<Cell, number[]>();
  for (const index of everyRow) {
    const key = keys[index] ?? null;
    const members = groups.get(key);
    if (members === undefined) {
      groups.set(key, [index]);
    } else {
      members.push(index);
    }
  }
  return [...groups.values()];
}

function applyWithin(method: Method, values: readonly Cell[], groups: readonly number[][]): Cell[] {
  const result: Cell[] = values.map(() => null);
  for (const members of groups) {
    const worked = applyAcrossGroup(
      method,
      members.map((index) => values[index] ?? null),
    );
    for (const [position, index] of members.entries()) {
      result[index] = worked[position] ?? null;
    }
  }
  return result;
}

/** Works `expr` out on the rows of `frame`, or returns null when it has a problem. */
export function evaluateExpr(expr: Expr, frame: Frame): readonly Cell[] | null {
  const column = findColumn(frame, expr.column);
  if (column === null || checkExpr(expr, frame).problem !== null) return null;
  let values: readonly Cell[] = column.cells;
  let waiting: Method[] = [];
  const rowCount = column.cells.length;
  for (const method of expr.methods) {
    if (method.name === 'over') {
      const keys = findColumn(frame, method.argument ?? '')?.cells ?? null;
      const groups = rowGroups(keys, rowCount);
      for (const earlier of waiting) {
        values = applyWithin(earlier, values, groups);
      }
      waiting = [];
    } else {
      waiting.push(method);
    }
  }
  const wholeFrame = rowGroups(null, rowCount);
  for (const earlier of waiting) {
    values = applyWithin(earlier, values, wholeFrame);
  }
  return values;
}

/** Returns the argument `name` starts with when it is added to a chain. */
export function defaultArgument(name: MethodName): string | null {
  switch (ARGUMENT_KINDS[name]) {
    case 'integer':
      return '1';
    case 'duration':
      return '-1d';
    case 'column':
      return 'dcn';
    default:
      return null;
  }
}

/** Returns `text` as a Python string literal. */
export function quoted(text: string): string {
  return JSON.stringify(text);
}

function methodText(method: Method): string {
  switch (ARGUMENT_KINDS[method.name]) {
    case 'integer':
      return `.${method.name}(${method.argument ?? ''})`;
    case 'duration':
    case 'column':
      return `.${method.name}(${quoted(method.argument ?? '')})`;
    default:
      return `.${method.name}()`;
  }
}

/** Returns the Polars text of `expr`. */
export function exprText(expr: Expr): string {
  const calls = expr.methods.map((method) => methodText(method)).join('');
  return `pl.col(${quoted(expr.column)})${calls}.alias(${quoted(expr.alias)})`;
}
