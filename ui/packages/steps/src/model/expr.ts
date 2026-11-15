/**
 * @file The methods of an expression, in Polars' own vocabulary. Knows what
 * each method returns, and applies a method to values. Which methods exist,
 * what they are called, which dtypes they take and what their argument is
 * come from the catalogue the host serves, not from here.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { EntryView, ParameterView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import {
  argumentOf,
  groupWords,
  groupsHold,
  methodEntry,
  methodId,
  methodNameOf,
} from './catalogue';
import type { Cell, Dtype, Frame } from './frame';
import {
  assertNever,
  compareCells,
  findColumn,
  isNumeric,
  isTemporal,
  literalDtype,
  quoted,
  supertype,
  withLiteral,
} from './frame';

/** The methods that compare a value with another and give a Boolean. */
export type ComparisonMethod = 'eq' | 'ne' | 'gt' | 'ge' | 'lt' | 'le';

/** Every expression method the storyboard can preview, by its Polars name. */
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

/** The methods the preview has logic for. Which of them are offered, and how, is the catalogue's. */
const PREVIEWED: Readonly<Record<MethodName, true>> = {
  shift: true,
  first: true,
  last: true,
  cum_sum: true,
  cum_count: true,
  cum_max: true,
  sub: true,
  'dt.date': true,
  'dt.time': true,
  'dt.offset_by': true,
  'dt.month_start': true,
  'dt.total_days': true,
  over: true,
  eq: true,
  ne: true,
  gt: true,
  ge: true,
  lt: true,
  le: true,
  is_null: true,
  is_not_null: true,
};

/** Reports whether the storyboard can preview the method called `name`. */
export function isMethodName(name: string): name is MethodName {
  return Object.hasOwn(PREVIEWED, name);
}

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
  return (
    name === 'eq' ||
    name === 'ne' ||
    name === 'gt' ||
    name === 'ge' ||
    name === 'lt' ||
    name === 'le'
  );
}

/** What an argument is, once looked up: a field of the rows, a number or some text. */
export type Given =
  | { readonly kind: 'field'; readonly name: string; readonly dtype: Dtype }
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'text'; readonly value: string };

/** An argument as the rows read it: what it is given, or why it is not yet. */
export type Reading =
  | Given
  | { readonly kind: 'unset' }
  | { readonly kind: 'missing'; readonly name: string };

/** Reads `argument` against the fields of `frame`. */
export function readArgument(argument: string | null, frame: Frame): Reading {
  const read = readComparand(argument);
  if (read.kind === 'unset') return { kind: 'unset' };
  if (read.kind === 'literal') {
    return typeof read.value === 'number'
      ? { kind: 'number', value: read.value }
      : { kind: 'text', value: read.value };
  }
  const column = findColumn(frame, read.column);
  return column === null
    ? { kind: 'missing', name: read.column }
    : { kind: 'field', name: column.name, dtype: column.dtype };
}

/** Reports whether `reading` is an argument that exists, rather than one still to be picked or missing. */
export function isGiven(reading: Reading): reading is Given {
  return reading.kind === 'field' || reading.kind === 'number' || reading.kind === 'text';
}

/**
 * Returns the dtype an argument has. A number written on its own is an Int32,
 * or an Int64 when an Int32 cannot hold it.
 */
export function givenDtype(given: Given): Dtype {
  switch (given.kind) {
    case 'field':
      return given.dtype;
    case 'number':
      return literalDtype(given.value);
    case 'text':
      return 'String';
    default:
      return assertNever(given);
  }
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
 * Returns the dtype a value of `input` dtype has less `given`, or null when
 * Polars does not subtract the two. A number written out takes the column's
 * dtype, widened when it does not fit.
 */
function subtractedGiven(input: Dtype, given: Given): Dtype | null {
  switch (given.kind) {
    case 'field':
      return subtractedDtype(input, given.dtype);
    case 'number':
      return isNumeric(input) ? withLiteral(input, given.value) : null;
    case 'text':
      return null;
    default:
      return assertNever(given);
  }
}

/**
 * Returns the dtype `name` produces from `input`, or null when the preview
 * does not take it. `given` is what the method works with, for a method that
 * takes an argument and when that is known.
 */
export function methodDtype(name: MethodName, input: Dtype, given: Given | null): Dtype | null {
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
      return isNumeric(input) || isTemporal(input) || input === 'String' || input === 'Boolean'
        ? input
        : null;
    case 'sub': {
      if (!isNumeric(input) && !isTemporal(input)) return null;
      // Without an argument that fits, the dtype is what a column like the input would give.
      const fitting = given === null ? null : subtractedGiven(input, given);
      return fitting ?? subtractedDtype(input, input);
    }
    case 'dt.total_days':
      return input === 'Duration' ? 'Int64' : null;
    case 'dt.date':
      return input === 'Datetime' || input === 'Date' ? 'Date' : null;
    case 'dt.time':
      return input === 'Datetime' || input === 'Time' ? 'Time' : null;
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

/** An expression method the catalogue offers and the storyboard can preview. */
export interface PreviewedMethod {
  readonly name: MethodName;
  readonly entry: EntryView;
}

/**
 * Returns the expression entries of `catalogue` whose method the storyboard
 * can preview, in the catalogue's order.
 */
export function previewedMethods(catalogue: Catalogue): PreviewedMethod[] {
  return catalogue.view.entries.flatMap((entry) => {
    const name = entry.applies === 'expression' ? methodNameOf(entry.id) : null;
    return name !== null && isMethodName(name) ? [{ name, entry }] : [];
  });
}

/** Why something does not fit: briefly, for a list, and as a sentence. */
export interface Misfit {
  readonly short: string;
  readonly message: string;
}

/**
 * Returns why the method `name` cannot be handed a value of `input` dtype, or
 * `null` when it can. The catalogue says which dtypes a method accepts; the
 * preview can still lack the logic for one the catalogue accepts.
 */
export function acceptanceMisfit(
  catalogue: Catalogue,
  name: MethodName,
  input: Dtype,
): Misfit | null {
  const entry = methodEntry(catalogue, name);
  if (entry === null) {
    return {
      short: 'not in the catalogue',
      message: `The catalogue has no entry ${methodId(name)}, so ${name} cannot be used.`,
    };
  }
  if (!groupsHold(entry.accepts, input)) {
    const words = groupWords(catalogue, entry.accepts);
    return {
      short: `needs ${words}`,
      message: `${entry.name} needs ${words}, but it is given ${input}.`,
    };
  }
  if (methodDtype(name, input, null) === null) {
    return {
      short: 'not previewed yet',
      message: `${entry.name} is not previewed on ${input} yet.`,
    };
  }
  return null;
}

/** What a method's argument is held to: its catalogue parameter and the name the method is shown by. */
export interface ArgumentRule {
  readonly name: MethodName;
  /** The name the catalogue shows the method by. */
  readonly shown: string;
  readonly parameter: ParameterView;
  /** The dtype groups the argument may have. */
  readonly dtypes: readonly string[];
}

/**
 * Returns the rule of the argument of the method `name`, or `null` when the
 * catalogue has no entry for it or its first parameter is not an expression.
 */
export function argumentRule(catalogue: Catalogue, name: MethodName): ArgumentRule | null {
  const entry = methodEntry(catalogue, name);
  const argument = entry === null ? null : argumentOf(entry);
  if (entry === null || argument === null) return null;
  return { name, shown: entry.name, ...argument };
}

/** Reports whether a column of `operand` dtype can be taken from a value of `input` dtype. */
function operandFits(input: Dtype, operand: Dtype): boolean {
  return subtractedDtype(input, operand) !== null;
}

function describe(given: Given): string {
  switch (given.kind) {
    case 'field':
      return `"${given.name}"`;
    case 'number':
      return String(given.value);
    case 'text':
      return quoted(given.value);
    default:
      return assertNever(given);
  }
}

function subtractionMisfit(input: Dtype, given: Given, different: Misfit): Misfit | null {
  if (given.kind === 'field') return operandFits(input, given.dtype) ? null : different;
  if (given.kind === 'text' || !isNumeric(input)) return different;
  if (withLiteral(input, given.value) !== null) return null;
  return {
    short: 'too large',
    message: `${String(given.value)} beside ${input} is UInt64, which this storyboard does not show.`,
  };
}

function comparisonMisfit(input: Dtype, given: Given, different: Misfit): Misfit | null {
  switch (given.kind) {
    case 'field':
      return supertype(input, given.dtype) === null ? different : null;
    case 'number':
      return isNumeric(input) ? null : different;
    case 'text':
      return input === 'String' ? null : different;
    default:
      return assertNever(given);
  }
}

/**
 * Returns why a value of `input` dtype and `given` do not go together for the
 * method `rule` is the argument of, or `null` when they do. The dtype groups
 * of the parameter come from the catalogue; what Polars does with the two
 * together is this preview's. `rows` is how many rows there are.
 */
export function argumentMisfit(
  catalogue: Catalogue,
  rule: ArgumentRule,
  input: Dtype,
  given: Given,
  rows: number,
): Misfit | null {
  const { shown } = rule;
  const dtype = givenDtype(given);
  const which = `${describe(given)} is ${dtype}`;
  if (!groupsHold(rule.dtypes, dtype)) {
    const words = groupWords(catalogue, rule.dtypes);
    return { short: `needs ${words}`, message: `${shown} takes ${words}, but ${which}.` };
  }
  const different = {
    short: 'a different dtype',
    message: `${shown} is given ${input}, but ${which}.`,
  };
  const { name } = rule;
  if (isComparison(name)) return comparisonMisfit(input, given, different);
  switch (name) {
    case 'sub':
      return subtractionMisfit(input, given, different);
    case 'shift':
      // Polars shifts by one number, not by one for each row.
      return given.kind === 'field' && rows !== 1
        ? {
            short: 'one for each row',
            message: `${shown} takes a single value for ${rule.parameter.shown}, but ${describe(given)} has one on each row.`,
          }
        : null;
    case 'dt.offset_by':
      return given.kind === 'text' && !DURATION.test(given.value)
        ? {
            short: 'not a duration',
            message: `${shown} takes a duration such as "-1d", "2w" or "1mo".`,
          }
        : null;
    default:
      return null;
  }
}

/** Returns why `method`'s argument is wrong for a value of `input` dtype, or `null` when it is right. */
export function argumentProblem(
  catalogue: Catalogue,
  method: Method,
  frame: Frame,
  input: Dtype,
): string | null {
  const rule = argumentRule(catalogue, method.name);
  if (rule === null) return null;
  const reading = readArgument(method.argument, frame);
  if (reading.kind === 'unset') {
    return `Give ${rule.shown} a value or a field (${rule.parameter.label}).`;
  }
  if (reading.kind === 'missing') return `There is no column "${reading.name}".`;
  return (
    argumentMisfit(catalogue, rule, input, reading, frame[0]?.cells.length ?? 0)?.message ?? null
  );
}

/** Returns `value` moved by `duration`, or null when `duration` is not one. */
function offsetBy(value: string, duration: string): string | null {
  const groups = DURATION.exec(duration)?.groups;
  if (groups === undefined) return null;
  const amount = Number(groups.amount);
  const moment = new Date(`${value.slice(0, 10)}T00:00:00Z`);
  if (groups.unit === 'mo') {
    // Polars keeps the day of the month, or the month's last day when it is shorter.
    const day = moment.getUTCDate();
    moment.setUTCDate(1);
    moment.setUTCMonth(moment.getUTCMonth() + amount);
    const lastDay = new Date(
      Date.UTC(moment.getUTCFullYear(), moment.getUTCMonth() + 1, 0),
    ).getUTCDate();
    moment.setUTCDate(Math.min(day, lastDay));
  } else {
    moment.setUTCDate(moment.getUTCDate() + amount * (groups.unit === 'w' ? 7 : 1));
  }
  return `${moment.toISOString().slice(0, 10)}${value.slice(10)}`;
}

/** Applies `method` to one value; `other` is what its argument gives on that row. */
function applyToEach(method: Method, value: Cell, other: Cell): Cell {
  if (typeof value !== 'string') return value;
  switch (method.name) {
    case 'dt.date':
      return value.slice(0, 10);
    case 'dt.time':
      // A time is kept as its hour and minute alone.
      return value.length === 5 ? value : value.slice(11, 16);
    case 'dt.offset_by':
      return typeof other === 'string' ? offsetBy(value, other) : null;
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
      // Every row gives the same number: a shift takes one, not one for each row.
      const rows = typeof operand[0] === 'number' ? operand[0] : 0;
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
      return values.map((value, position) => applyToEach(method, value, operand[position] ?? null));
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

/**
 * Returns what the argument of `method` gives on each of `rowCount` rows: the
 * cells of a field, or one value on every row. A method with no argument, or
 * with one still to be picked, is given nothing.
 */
function otherSide(method: Method, rowCount: number, frame: Frame): readonly Cell[] {
  const reading = readArgument(method.argument, frame);
  switch (reading.kind) {
    case 'field':
      return findColumn(frame, reading.name)?.cells ?? [];
    case 'number':
    case 'text':
      return Array.from({ length: rowCount }, () => reading.value);
    default:
      return [];
  }
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

const PYTHON_NUMBER = /^-?\d+$/;
const PYTHON_TEXT = /^'(?<text>[^']*)'$/;

/**
 * Returns the argument `name` starts with when it is added to a chain, or
 * `null` when it takes none. A default Polars gives as a whole number or as
 * text is that value, when it fits the parameter. Anything else is still to
 * be picked, and so is the empty argument.
 */
export function defaultArgument(catalogue: Catalogue, name: MethodName): string | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  const python = rule.parameter.default;
  if (python === null) return '';
  if (PYTHON_NUMBER.test(python)) {
    const value = Number(python);
    return groupsHold(rule.dtypes, literalDtype(value)) ? literalComparand(value) : '';
  }
  const text = PYTHON_TEXT.exec(python)?.groups?.text;
  return text !== undefined && groupsHold(rule.dtypes, 'String') ? literalComparand(text) : '';
}

/**
 * Returns the argument `name` has when it replaces `method`: the one the
 * method had when it fits the new parameter's dtypes, and the default
 * otherwise.
 */
export function swappedArgument(
  catalogue: Catalogue,
  method: Method,
  name: MethodName,
  frame: Frame,
): string | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  const reading = readArgument(method.argument, frame);
  const fits = isGiven(reading) && groupsHold(rule.dtypes, givenDtype(reading));
  return fits ? method.argument : defaultArgument(catalogue, name);
}

/** Returns the Polars text of an argument: a field as `pl.col`, a number as it is, text as `pl.lit`. */
function argumentText(argument: string | null): string {
  const read = readComparand(argument);
  if (read.kind === 'column') return `pl.col(${quoted(read.column)})`;
  if (read.kind === 'unset') return 'None';
  return typeof read.value === 'number' ? String(read.value) : `pl.lit(${quoted(read.value)})`;
}

/** Returns the Polars text of one method call. A method with no argument is written with none. */
export function methodText(method: Method): string {
  return `.${method.name}(${method.argument === null ? '' : argumentText(method.argument)})`;
}
