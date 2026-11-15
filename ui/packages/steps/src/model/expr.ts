/**
 * @file The methods of an expression, in Polars' own vocabulary. Knows which
 * dtype each method accepts and returns, and applies a method to values.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Cell, Dtype, Frame } from './frame';
import { assertNever, compareCells, findColumn, isNumeric, quoted, supertype } from './frame';

/** The methods that compare a value with another and give a Boolean. */
export type ComparisonMethod = 'eq' | 'ne' | 'gt' | 'ge' | 'lt' | 'le';

/** Every expression method the prototype knows, by its Polars name. */
export type MethodName =
  | 'shift'
  | 'first'
  | 'last'
  | 'cum_sum'
  | 'cum_count'
  | 'cum_max'
  | 'sub'
  | 'dt.date'
  | 'dt.time'
  | 'dt.offset_by'
  | 'dt.month_start'
  | 'dt.total_days'
  | 'over'
  | ComparisonMethod
  | 'is_null'
  | 'is_not_null';

/** Every method, in the order it is offered. */
export const METHOD_NAMES: readonly MethodName[] = [
  'shift',
  'first',
  'last',
  'cum_sum',
  'cum_count',
  'cum_max',
  'sub',
  'dt.date',
  'dt.time',
  'dt.offset_by',
  'dt.month_start',
  'dt.total_days',
  'over',
  'eq',
  'ne',
  'gt',
  'ge',
  'lt',
  'le',
  'is_null',
  'is_not_null',
];

/** A group of methods that can stand in for one another. */
export type MethodGroup = 'rows' | 'cumulative' | 'arithmetic' | 'dt' | 'window' | 'compare';

/** Every group, in the order it is offered. */
export const METHOD_GROUP_ORDER: readonly MethodGroup[] = [
  'rows',
  'cumulative',
  'arithmetic',
  'dt',
  'window',
  'compare',
];

/** Headings for the groups. */
export const METHOD_GROUP_TITLES: Readonly<Record<MethodGroup, string>> = {
  rows: 'Rows',
  cumulative: 'Cumulative',
  arithmetic: 'Arithmetic',
  dt: 'dt · dates and times',
  window: 'Window',
  compare: 'Comparison · true or false',
};

/** The group each method belongs to. */
export const METHOD_GROUPS: Readonly<Record<MethodName, MethodGroup>> = {
  shift: 'rows',
  first: 'rows',
  last: 'rows',
  cum_sum: 'cumulative',
  cum_count: 'cumulative',
  cum_max: 'cumulative',
  sub: 'arithmetic',
  'dt.date': 'dt',
  'dt.time': 'dt',
  'dt.offset_by': 'dt',
  'dt.month_start': 'dt',
  'dt.total_days': 'dt',
  over: 'window',
  eq: 'compare',
  ne: 'compare',
  gt: 'compare',
  ge: 'compare',
  lt: 'compare',
  le: 'compare',
  is_null: 'compare',
  is_not_null: 'compare',
};

/** What each method does, for someone who does not know the name yet. */
export const METHOD_HINTS: Readonly<Record<MethodName, string>> = {
  shift: 'value n rows earlier',
  first: 'first value',
  last: 'last value',
  cum_sum: 'running total',
  cum_count: 'running count',
  cum_max: 'running maximum',
  sub: 'minus another column',
  'dt.date': 'the date part',
  'dt.time': 'the time part',
  'dt.offset_by': 'move by a duration',
  'dt.month_start': 'first of the month',
  'dt.total_days': 'whole days in a duration',
  over: 'within each group',
  eq: 'equal to',
  ne: 'not equal to',
  gt: 'greater than',
  ge: 'at least',
  lt: 'less than',
  le: 'at most',
  is_null: 'has no value',
  is_not_null: 'has a value',
};

/**
 * What a method's one argument is, or `null` when it takes none. A comparand
 * is what a comparison is given: a field, or a value written out.
 */
export type ArgumentKind = 'integer' | 'duration' | 'column' | 'operand' | 'comparand';

/** The kind of argument each method takes. */
export const ARGUMENT_KINDS: Readonly<Record<MethodName, ArgumentKind | null>> = {
  shift: 'integer',
  first: null,
  last: null,
  cum_sum: null,
  cum_count: null,
  cum_max: null,
  sub: 'operand',
  'dt.date': null,
  'dt.time': null,
  'dt.offset_by': 'duration',
  'dt.month_start': null,
  'dt.total_days': null,
  over: 'column',
  eq: 'comparand',
  ne: 'comparand',
  gt: 'comparand',
  ge: 'comparand',
  lt: 'comparand',
  le: 'comparand',
  is_null: null,
  is_not_null: null,
};

/** One method call. `argument` holds the argument as typed, or `null`. */
export interface Method {
  /** Stays with the call while it is moved or edited. */
  readonly id: string;
  readonly name: MethodName;
  readonly argument: string | null;
}

const DURATION = /^(?<amount>-?\d+)(?<unit>d|w|mo)$/;
const INTEGER = /^-?\d+$/;

/** What a comparison is given: a field, a number or text, or nothing yet. */
export type Comparand =
  | { readonly kind: 'column'; readonly column: string }
  | { readonly kind: 'literal'; readonly value: string | number }
  | { readonly kind: 'unset' };

/**
 * Reads the argument of a comparison. Digits are a number, text between
 * double quotation marks is that text, nothing is nothing yet, and anything
 * else names a field. So a field whose name is all digits, or starts with a
 * quotation mark, cannot be compared with here.
 */
export function readComparand(argument: string | null): Comparand {
  const text = argument ?? '';
  if (text === '') return { kind: 'unset' };
  if (INTEGER.test(text)) return { kind: 'literal', value: Number(text) };
  if (text.length >= 2 && text.startsWith('"') && text.endsWith('"')) {
    return { kind: 'literal', value: text.slice(1, -1) };
  }
  return { kind: 'column', column: text };
}

/** Writes a number or text as the argument of a comparison, for `readComparand` to read back. */
export function literalComparand(value: string | number): string {
  return typeof value === 'number' ? String(value) : `"${value}"`;
}

function isComparison(name: MethodName): name is ComparisonMethod {
  return ARGUMENT_KINDS[name] === 'comparand';
}

function isTemporal(dtype: Dtype): boolean {
  return dtype === 'Datetime' || dtype === 'Date' || dtype === 'Time' || dtype === 'Duration';
}

/** Reports whether `dtype` is a point in time: a day, or a day and a time. */
function isMoment(dtype: Dtype): boolean {
  return dtype === 'Datetime' || dtype === 'Date';
}

/**
 * Returns the dtype of a value of `input` dtype less a column of `operand`
 * dtype, or null when Polars does not subtract the two. Numbers meet in
 * their supertype. Two moments, two times or two durations give the Duration
 * between them. A moment less a Duration is an earlier moment.
 */
function subtractedDtype(input: Dtype, operand: Dtype): Dtype | null {
  if (isNumeric(input)) return isNumeric(operand) ? supertype(input, operand) : null;
  if (isMoment(input)) {
    if (isMoment(operand)) return 'Duration';
    return operand === 'Duration' ? input : null;
  }
  if (input === 'Time' || input === 'Duration') return operand === input ? 'Duration' : null;
  return null;
}

/**
 * Returns the dtype `name` produces from `input`, or null when it does not
 * accept it. `operand` is the dtype of the column the method works with, for
 * a method that takes one and when that column is known.
 */
export function methodDtype(name: MethodName, input: Dtype, operand: Dtype | null): Dtype | null {
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
      return isNumeric(input) || input === 'Duration' ? input : null;
    case 'cum_max':
      return isNumeric(input) || isTemporal(input) ? input : null;
    case 'sub': {
      if (!isNumeric(input) && !isTemporal(input)) return null;
      // Without a column that fits, the dtype is what a column like the input would give.
      const fitting = operand === null ? null : subtractedDtype(input, operand);
      return fitting ?? subtractedDtype(input, input);
    }
    case 'dt.total_days':
      return input === 'Duration' ? 'Int64' : null;
    case 'dt.date':
      return input === 'Datetime' ? 'Date' : null;
    case 'dt.time':
      return input === 'Datetime' ? 'Time' : null;
    case 'dt.offset_by':
    case 'dt.month_start':
      return input === 'Datetime' || input === 'Date' ? input : null;
    case 'eq':
    case 'ne':
    case 'gt':
    case 'ge':
    case 'lt':
    case 'le':
    case 'is_null':
    case 'is_not_null':
      return 'Boolean';
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
    case 'eq':
    case 'ne':
    case 'gt':
    case 'ge':
    case 'lt':
    case 'le':
    case 'is_null':
    case 'is_not_null':
      return 'any dtype';
    case 'cum_sum':
      return 'a numeric dtype or Duration';
    case 'cum_max':
    case 'sub':
      return 'a numeric or temporal dtype';
    case 'dt.total_days':
      return 'Duration';
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

/** Reports whether a column of `operand` dtype can be combined with a value of `input` dtype. */
export function operandFits(input: Dtype, operand: Dtype): boolean {
  return subtractedDtype(input, operand) !== null;
}

function operandProblem(method: Method, frame: Frame, input: Dtype): string | null {
  const column = findColumn(frame, method.argument ?? '');
  if (column === null) return `${method.name} needs a column to work with.`;
  return operandFits(input, column.dtype)
    ? null
    : `${method.name} is given ${input}, but "${column.name}" is ${column.dtype}.`;
}

function comparandProblem(method: Method, frame: Frame, input: Dtype): string | null {
  const comparand = readComparand(method.argument);
  if (comparand.kind === 'unset') return `${method.name} needs a value or a field to compare with.`;
  if (comparand.kind === 'column') {
    const column = findColumn(frame, comparand.column);
    if (column === null) return `There is no column "${comparand.column}".`;
    return supertype(input, column.dtype) === null
      ? `${method.name} is given ${input}, but "${column.name}" is ${column.dtype}.`
      : null;
  }
  if (typeof comparand.value === 'number') {
    return isNumeric(input)
      ? null
      : `${method.name} is given ${input}, but ${String(comparand.value)} is a number.`;
  }
  return input === 'String'
    ? null
    : `${method.name} is given ${input}, but ${quoted(comparand.value)} is text.`;
}

/** Returns why `method`'s argument is wrong for a value of `input` dtype, or `null` when it is right. */
export function argumentProblem(method: Method, frame: Frame, input: Dtype): string | null {
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
    case 'operand':
      return operandProblem(method, frame, input);
    case 'comparand':
      return comparandProblem(method, frame, input);
    default:
      return assertNever(kind);
  }
}

function offsetBy(value: string, duration: string): string {
  const groups = DURATION.exec(duration)?.groups;
  const amount = Number(groups?.amount ?? 0);
  const moment = new Date(`${value.slice(0, 10)}T00:00:00Z`);
  if (groups?.unit === 'mo') {
    // Polars keeps the day of the month, or the month's last day when it is shorter.
    const day = moment.getUTCDate();
    moment.setUTCDate(1);
    moment.setUTCMonth(moment.getUTCMonth() + amount);
    const lastDay = new Date(
      Date.UTC(moment.getUTCFullYear(), moment.getUTCMonth() + 1, 0),
    ).getUTCDate();
    moment.setUTCDate(Math.min(day, lastDay));
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

/** Returns the highest value so far at each row. A null stays null and does not count. */
function runningMaximum(values: readonly Cell[]): Cell[] {
  let highest: Cell = null;
  return values.map((value) => {
    if (value === null) return null;
    if (highest === null || compareCells(value, highest) > 0) highest = value;
    return highest;
  });
}

/**
 * Returns the minutes a time of day, a date or a date and time kept as text
 * stands for: since midnight for a time, since the epoch for the others.
 */
function minutesOf(value: string): number {
  if (value.length === 5) return Number(value.slice(0, 2)) * 60 + Number(value.slice(3, 5));
  const time = value.length > 10 ? `${value.slice(11, 16)}:00` : '00:00:00';
  return Date.parse(`${value.slice(0, 10)}T${time}Z`) / 60_000;
}

/** Writes minutes since the epoch as a date, or as a date and time when `withTime` is set. */
function momentText(minutes: number, withTime: boolean): string {
  const text = new Date(minutes * 60_000).toISOString();
  return withTime ? `${text.slice(0, 10)} ${text.slice(11, 16)}` : text.slice(0, 10);
}

/**
 * Subtracts one cell from another. Numbers are subtracted as numbers. Two
 * moments or two times are the minutes between them. A moment less a number
 * of minutes is the moment that long before it.
 */
function difference(left: Cell, right: Cell): Cell {
  if (left === null || right === null) return null;
  if (typeof left === 'number') return typeof right === 'number' ? left - right : null;
  if (typeof right === 'number') return momentText(minutesOf(left) - right, left.length > 10);
  return minutesOf(left) - minutesOf(right);
}

/** Returns a Boolean as a cell holds it. */
function booleanCell(value: boolean): Cell {
  return value ? 'true' : 'false';
}

/** Compares two cells as `name` does. A comparison with a null is null. */
function compared(name: ComparisonMethod, left: Cell, right: Cell): Cell {
  if (left === null || right === null) return null;
  const order = compareCells(left, right);
  switch (name) {
    case 'eq':
      return booleanCell(order === 0);
    case 'ne':
      return booleanCell(order !== 0);
    case 'gt':
      return booleanCell(order > 0);
    case 'ge':
      return booleanCell(order >= 0);
    case 'lt':
      return booleanCell(order < 0);
    case 'le':
      return booleanCell(order <= 0);
    default:
      return assertNever(name);
  }
}

function applyAcrossGroup(
  method: Method,
  values: readonly Cell[],
  operand: readonly Cell[],
): Cell[] {
  const { name } = method;
  if (isComparison(name)) {
    return values.map((value, position) => compared(name, value, operand[position] ?? null));
  }
  switch (method.name) {
    case 'is_null':
      return values.map((value) => booleanCell(value === null));
    case 'is_not_null':
      return values.map((value) => booleanCell(value !== null));
    case 'sub':
      return values.map((value, position) => difference(value, operand[position] ?? null));
    case 'dt.total_days':
      return values.map((value) => (typeof value === 'number' ? Math.trunc(value / 1440) : null));
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
      // A null stays null and adds nothing to the total.
      let total = 0;
      return values.map((value) => {
        if (typeof value !== 'number') return null;
        total += value;
        return total;
      });
    }
    case 'cum_max':
      return runningMaximum(values);
    default:
      return values.map((value) => applyToEach(method, value));
  }
}

/** Returns the row numbers of each group of equal `keys`, or one group of every row. */
export function rowGroups(keys: readonly Cell[] | null, rowCount: number): number[][] {
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

/** Returns what a two-sided method is given on each of `rowCount` rows, or nothing for another method. */
function otherSide(method: Method, rowCount: number, frame: Frame): readonly Cell[] {
  const kind = ARGUMENT_KINDS[method.name];
  if (kind === 'operand') return findColumn(frame, method.argument ?? '')?.cells ?? [];
  if (kind !== 'comparand') return [];
  const comparand = readComparand(method.argument);
  if (comparand.kind === 'column') return findColumn(frame, comparand.column)?.cells ?? [];
  const value = comparand.kind === 'literal' ? comparand.value : null;
  return Array.from({ length: rowCount }, () => value);
}

/** Applies `method` to `values` separately within each of `groups`. */
export function applyWithin(
  method: Method,
  values: readonly Cell[],
  groups: readonly number[][],
  frame: Frame,
): Cell[] {
  const result: Cell[] = values.map(() => null);
  // The other side of a two-sided method, row for row.
  const other = otherSide(method, values.length, frame);
  for (const members of groups) {
    const worked = applyAcrossGroup(
      method,
      members.map((index) => values[index] ?? null),
      members.map((index) => other[index] ?? null),
    );
    for (const [position, index] of members.entries()) {
      result[index] = worked[position] ?? null;
    }
  }
  return result;
}

/**
 * Returns the argument `name` starts with when it is added to a chain that
 * hands it `input` on `frame`.
 */
export function defaultArgument(
  name: MethodName,
  frame: Frame,
  input: Dtype | null,
): string | null {
  switch (ARGUMENT_KINDS[name]) {
    case 'operand':
      return frame.find((column) => input !== null && operandFits(input, column.dtype))?.name ?? '';
    case 'integer':
      return '1';
    case 'duration':
      return '-1d';
    case 'column':
      return frame[0]?.name ?? '';
    case 'comparand':
      // What it is compared with is still to be picked.
      return '';
    default:
      return null;
  }
}

/** Returns the Polars text of one method call. */
export function methodText(method: Method): string {
  switch (ARGUMENT_KINDS[method.name]) {
    case 'integer':
      return `.${method.name}(${method.argument ?? ''})`;
    case 'duration':
    case 'column':
      return `.${method.name}(${quoted(method.argument ?? '')})`;
    case 'operand':
      return `.${method.name}(pl.col(${quoted(method.argument ?? '')}))`;
    case 'comparand': {
      const comparand = readComparand(method.argument);
      if (comparand.kind === 'column') {
        return `.${method.name}(pl.col(${quoted(comparand.column)}))`;
      }
      if (comparand.kind === 'unset') return `.${method.name}(None)`;
      const { value } = comparand;
      return `.${method.name}(${typeof value === 'number' ? String(value) : quoted(value)})`;
    }
    default:
      return `.${method.name}()`;
  }
}
