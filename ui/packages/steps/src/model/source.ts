/**
 * @file Chains and what they are made of. A chain starts from a source, a
 * column or a condition that picks between values (`when … then …
 * otherwise`), and applies methods to it. A value inside a condition can
 * itself be a chain, so chains nest.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { EntryView, KindView, ParameterView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import { methodEntry } from './catalogue';
import type { Method } from './expr';
import {
  acceptanceMisfit,
  applyWithin,
  argumentProblem,
  emptyAggregate,
  fillProblem,
  isAggregating,
  isGiven,
  methodDtype,
  methodText,
  readArgument,
  rowGroups,
} from './expr';
import type { Cell, Dtype, Frame } from './frame';
import {
  assertNever,
  castCells,
  compareCells,
  findColumn,
  isNumeric,
  keysFit,
  literalDtype,
  quoted,
  supertype,
  withLiteral,
} from './frame';
import { findInput } from './inputs';
import type { Options, Setting } from './settings';
import { atDefaultOver, defaultSetting, isComplete, setParameters } from './settings';

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

/** A value that is still to be picked. It stands for nothing until it is. */
export interface UnsetValue {
  readonly kind: 'unset';
}

/** A value a condition compares with or returns. */
export type Value = ColumnValue | LiteralValue | ChainValue | InputValue | UnsetValue;

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

/** A condition written as a chain of its own, which must give a Boolean. */
export interface ChainCondition {
  readonly kind: 'chain';
  /** Stays with the chain while it is edited. */
  readonly id: string;
  readonly chain: Chain;
}

/**
 * What a filter keeps rows by, and what a `when` decides by. A group with no
 * items is a condition that is still to be picked.
 */
export type Condition = Test | Group | ChainCondition;

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

/**
 * A chain that starts from `pl.element()`: the values that land in one cell of
 * a pivot, which only the aggregate of a pivot has.
 */
export interface ElementSource {
  readonly kind: 'element';
}

/** What a chain starts from. */
export type Source = ColumnSource | WhenSource | ElementSource;

/** Where `pl.element()` is meant: the dtype of the values of one pivot cell, or `null` when that is not known yet. */
export interface ElementScope {
  readonly dtype: Dtype | null;
}

/** A source followed by method calls. */
export interface Chain {
  readonly source: Source;
  readonly methods: readonly Method[];
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

/** Returns the dtype `value` gives on `frame`, or why it gives none. */
export function checkValue(
  catalogue: Catalogue,
  value: Value,
  frame: Frame,
  element: ElementScope | null = null,
): SourceCheck {
  if (value.kind === 'unset') return { dtype: null, problem: 'Pick a value.' };
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
    const check = checkChain(catalogue, value.chain, frame, element);
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
export function valueText(catalogue: Catalogue, value: Value): string {
  if (value.kind === 'chain') return chainText(catalogue, value.chain);
  // What is still to be picked is written as `None`, which is what Polars would be given.
  if (value.kind === 'unset') return 'None';
  // An input is named in the code as it is in the editor.
  if (value.kind === 'input') return value.name;
  return value.kind === 'column' ? `pl.col(${quoted(value.column)})` : literalText(value.value);
}

/** Returns the Polars text of `value` as an expression, where a bare value would not be one. */
export function valueExprText(catalogue: Catalogue, value: Value): string {
  return value.kind === 'column' || value.kind === 'chain'
    ? valueText(catalogue, value)
    : `pl.lit(${valueText(catalogue, value)})`;
}

/**
 * Returns the Polars text of `value` as a key, such as a field to sort by: a
 * field is its name, which is how Python takes a column, and anything else is
 * an expression.
 */
export function keyText(catalogue: Catalogue, value: Value): string {
  return value.kind === 'column' ? quoted(value.column) : valueExprText(catalogue, value);
}

/** Returns the cell `value` gives on every row of `frame`. */
export function cellsOf(catalogue: Catalogue, value: Value, frame: Frame): readonly Cell[] {
  if (value.kind === 'chain') return evaluateChain(catalogue, value.chain, frame) ?? [];
  if (value.kind === 'column') return findColumn(frame, value.column)?.cells ?? [];
  if (value.kind === 'input') {
    const input = findInput(value.name);
    return rowsOf(frame).map(() => (input?.kind === 'value' ? input.value : null));
  }
  return rowsOf(frame).map(() => (value.kind === 'unset' ? null : value.value));
}

/** Reports whether two dtypes can be compared or returned side by side. */
export function dtypesAgree(left: Dtype, right: Dtype): boolean {
  return supertype(left, right) !== null;
}

/** Returns why `condition` cannot be worked out on `frame`, or `null` when it can. */
export function conditionProblem(
  catalogue: Catalogue,
  condition: Condition,
  frame: Frame,
): string | null {
  if (condition.kind === 'group') {
    if (condition.items.length === 0) return 'Pick a condition.';
    for (const item of condition.items) {
      const problem = conditionProblem(catalogue, item, frame);
      if (problem !== null) return problem;
    }
    return null;
  }
  if (condition.kind === 'chain') {
    const check = checkChain(catalogue, condition.chain, frame);
    if (check.problem !== null) return check.problem.message;
    const dtype = check.dtypes.at(-1);
    return dtype === 'Boolean'
      ? null
      : `A condition is true or false, but this chain gives ${dtype ?? 'nothing'}. End it with a comparison.`;
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
  const value = checkValue(catalogue, condition.value, frame);
  if (value.problem !== null) return value.problem;
  if (value.dtype !== null && !dtypesAgree(column.dtype, value.dtype)) {
    return `"${column.name}" is ${column.dtype}, but ${valueText(catalogue, condition.value)} is ${value.dtype}.`;
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
export function conditionRows(catalogue: Catalogue, condition: Condition, frame: Frame): boolean[] {
  if (condition.kind === 'group') {
    const items = condition.items.map((item) => conditionRows(catalogue, item, frame));
    return rowsOf(frame).map((row) =>
      condition.every
        ? items.every((item) => item[row] === true)
        : items.some((item) => item[row] === true),
    );
  }
  if (condition.kind === 'chain') {
    const cells = evaluateChain(catalogue, condition.chain, frame) ?? [];
    return rowsOf(frame).map((row) => cells[row] === 'true');
  }
  const left = findColumn(frame, condition.column)?.cells ?? [];
  if (condition.comparison === 'is_in') {
    const list = condition.value.kind === 'input' ? findInput(condition.value.name) : null;
    const members = list?.kind === 'list' ? list.values : [];
    return rowsOf(frame).map((row) => left[row] !== null && members.includes(left[row] ?? null));
  }
  // Both sides are compared as the dtype they meet in.
  const column = findColumn(frame, condition.column)?.dtype ?? null;
  const value = checkValue(catalogue, condition.value, frame).dtype;
  const met = column !== null && value !== null ? supertype(column, value) : null;
  const compared = castCells(left, column, met);
  const right = castCells(cellsOf(catalogue, condition.value, frame), value, met);
  return rowsOf(frame).map((row) =>
    holds(condition.comparison, compared[row] ?? null, right[row] ?? null),
  );
}

/** Returns the Polars text of `condition`. */
export function conditionText(catalogue: Catalogue, condition: Condition): string {
  if (condition.kind === 'group') {
    // Every item stands in brackets, so the text says what the lists say.
    const joined = condition.items.map((item) => `(${conditionText(catalogue, item)})`);
    return joined.join(condition.every ? ' & ' : ' | ');
  }
  if (condition.kind === 'chain') return chainText(catalogue, condition.chain);
  if (condition.comparison === 'is_in') {
    return `pl.col(${quoted(condition.column)}).is_in(${valueText(catalogue, condition.value)})`;
  }
  return `pl.col(${quoted(condition.column)}) ${condition.comparison} ${valueText(catalogue, condition.value)}`;
}

/** Returns the Polars text of a value passed to `then` or `otherwise`. */
function returnedText(catalogue: Catalogue, value: Value): string {
  // A bare string there would be read as a column name.
  return value.kind === 'literal' && typeof value.value === 'string'
    ? `pl.lit(${quoted(value.value)})`
    : valueText(catalogue, value);
}

const STRING: SourceCheck = { dtype: 'String', problem: null };

function differing(first: Dtype, second: Dtype): SourceCheck {
  return {
    dtype: null,
    problem: `then and otherwise return different dtypes: ${first} and ${second}.`,
  };
}

function checkWhen(catalogue: Catalogue, source: WhenSource, frame: Frame): SourceCheck {
  for (const arm of source.arms) {
    const problem = conditionProblem(catalogue, arm.condition, frame);
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
    const checked = checkValue(catalogue, value, frame);
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

/** Returns the dtype `source` gives on `frame`, or why it gives none. `element` is where `pl.element()` is meant, if anywhere. */
export function checkSource(
  catalogue: Catalogue,
  source: Source,
  frame: Frame,
  element: ElementScope | null = null,
): SourceCheck {
  if (source.kind === 'when') return checkWhen(catalogue, source, frame);
  if (source.kind === 'element') {
    if (element === null) {
      return {
        dtype: null,
        problem: 'element stands for the values of one cell of a pivot, so only a pivot takes it.',
      };
    }
    return element.dtype === null
      ? { dtype: null, problem: 'Pick the field the pivot takes its values from first.' }
      : { dtype: element.dtype, problem: null };
  }
  if (source.column === '') {
    return { dtype: null, problem: 'Pick the field the chain starts from.' };
  }
  const column = findColumn(frame, source.column);
  return column === null
    ? { dtype: null, problem: `There is no column "${source.column}".` }
    : { dtype: column.dtype, problem: null };
}

/** Works `source` out on the rows of `frame`. */
function evaluateSource(catalogue: Catalogue, source: Source, frame: Frame): readonly Cell[] {
  // The values of one pivot cell are given where the pivot works them out.
  if (source.kind === 'element') return [];
  if (source.kind === 'column') return findColumn(frame, source.column)?.cells ?? [];
  // Every value is returned as the dtype they all meet in.
  const result = checkWhen(catalogue, source, frame).dtype;
  const returned = (value: Value): readonly Cell[] =>
    castCells(cellsOf(catalogue, value, frame), checkValue(catalogue, value, frame).dtype, result);
  const arms = source.arms.map((arm) => ({
    holds: conditionRows(catalogue, arm.condition, frame),
    cells: returned(arm.returns),
  }));
  const otherwise = returned(source.otherwise);
  return rowsOf(frame).map((row) => {
    const arm = arms.find((candidate) => candidate.holds[row] === true);
    return (arm === undefined ? otherwise[row] : arm.cells[row]) ?? null;
  });
}

/** Returns the Polars text `source` starts a chain with. */
function sourceText(catalogue: Catalogue, source: Source): string {
  if (source.kind === 'column') return `pl.col(${quoted(source.column)})`;
  if (source.kind === 'element') return 'pl.element()';
  const arms = source.arms
    .map(
      (arm) =>
        `.when(${conditionText(catalogue, arm.condition)}).then(${returnedText(catalogue, arm.returns)})`,
    )
    .join('');
  return `pl${arms}.otherwise(${returnedText(catalogue, source.otherwise)})`;
}

/** Checks that the source of `chain` works and every method accepts the dtype handed to it. */
export function checkChain(
  catalogue: Catalogue,
  chain: Chain,
  frame: Frame,
  element: ElementScope | null = null,
): ChainCheck {
  const start = checkSource(catalogue, chain.source, frame, element);
  if (start.problem !== null || start.dtype === null) {
    const message = start.problem ?? 'This chain has no dtype.';
    return { dtypes: [], problem: { index: -1, message } };
  }
  const dtypes: Dtype[] = [start.dtype];
  let current = start.dtype;
  for (const [index, method] of chain.methods.entries()) {
    const refused = acceptanceMisfit(catalogue, method.name, current);
    if (refused !== null) return { dtypes, problem: { index, message: refused.message } };
    const reading = readArgument(method.argument, frame);
    const next = methodDtype(method.name, current, isGiven(reading) ? reading : null);
    if (next === null) {
      return { dtypes, problem: { index, message: `${method.name} gives no dtype here.` } };
    }
    const wrongArgument =
      argumentProblem(catalogue, method, frame, current) ??
      fillProblem(method, current) ??
      optionsProblem(catalogue, methodEntry(catalogue, method.name), method.options, frame);
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
export function evaluateChain(
  catalogue: Catalogue,
  chain: Chain,
  frame: Frame,
): readonly Cell[] | null {
  const check = checkChain(catalogue, chain, frame);
  if (check.problem !== null) return null;
  let values = evaluateSource(catalogue, chain.source, frame);
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
      // A value written out is the same on every row, so it puts every row in one group.
      const reading = readArgument(method.argument, frame);
      const keys =
        reading.kind === 'field' ? (findColumn(frame, reading.name)?.cells ?? null) : null;
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

/**
 * Works the chain `chain` of the values that land in one pivot cell, `cells`,
 * out to the one value it gives, or returns `null` when it does not end in a
 * method that gives one. No cells give what the last method gives no values.
 */
export function evaluateElement(
  chain: Chain,
  cells: readonly Cell[],
  frame: Frame,
): Cell | undefined {
  const last = chain.methods.at(-1);
  if (last === undefined || !isAggregating(last.name)) return undefined;
  if (cells.length === 0) return emptyAggregate(last.name);
  const everyRow = [Array.from(cells, (_cell, row) => row)];
  let values: readonly Cell[] = cells;
  for (const method of chain.methods) values = applyWithin(method, values, everyRow, frame);
  return values[0] ?? null;
}

/** Returns the Polars text of `chain`. */
export function chainText(catalogue: Catalogue, chain: Chain): string {
  const methods = chain.methods.map((method) => methodCall(catalogue, method));
  return `${sourceText(catalogue, chain.source)}${methods.join('')}`;
}

/** Returns the Polars text of an expression given as an argument: a field, a number or text, a chain. */
function argumentValueText(catalogue: Catalogue, value: Value): string {
  if (value.kind === 'literal' && typeof value.value === 'string') {
    return `pl.lit(${quoted(value.value)})`;
  }
  return valueText(catalogue, value);
}

/** Returns the Polars text of `setting` as a parameter of `kind` holds it. */
function settingText(
  catalogue: Catalogue,
  kind: KindView,
  setting: Setting,
  items: readonly string[],
  parameter: ParameterView,
): string {
  switch (setting.kind) {
    case 'unset':
      return 'None';
    case 'flag':
      return setting.value ? 'True' : 'False';
    case 'number':
      return String(setting.value);
    case 'text':
      return quoted(setting.value);
    case 'word':
      if (kind.kind === 'dtype') return `pl.${setting.value}`;
      return kind.kind === 'table' ? setting.value : quoted(setting.value);
    case 'expression':
      return argumentValueText(catalogue, setting.value);
    case 'list':
      return `[${setting.items.map((item) => settingText(catalogue, kind, item, items, parameter)).join(', ')}]`;
    case 'pairs': {
      const key = kind.kind === 'pairs' ? kind.key : kind;
      const value = kind.kind === 'pairs' ? kind.value : kind;
      const pairs = setting.entries.map(
        (entry) =>
          `${settingText(catalogue, key, entry.key, [], parameter)}: ${settingText(catalogue, value, entry.value, [], parameter)}`,
      );
      return `{${pairs.join(', ')}}`;
    }
    case 'each': {
      // Every item has its place in the list; one with no setting of its own is at the default.
      const fallback = defaultSetting(parameter);
      const each = items.map((item) => {
        const own = Object.hasOwn(setting.items, item) ? setting.items[item] : undefined;
        const one = own ?? fallback;
        return one === null || one === undefined
          ? 'None'
          : settingText(catalogue, kind, one, [], parameter);
      });
      return `[${each.join(', ')}]`;
    }
    default:
      return assertNever(setting);
  }
}

/** Returns the Polars text of what `parameter` holds when it is set to `setting`, without its name. */
export function settingValueText(
  catalogue: Catalogue,
  parameter: ParameterView,
  setting: Setting,
  items: readonly string[],
): string {
  return settingText(catalogue, parameter.kind, setting, items, parameter);
}

/**
 * Returns how `parameter` is written in a call when it is set to `setting`.
 * Extra positional arguments are written as they are and extra keyword
 * arguments as a dictionary of them; anything else is named.
 */
function parameterText(
  catalogue: Catalogue,
  parameter: ParameterView,
  setting: Setting,
  items: readonly string[],
): string {
  const text = settingText(catalogue, parameter.kind, setting, items, parameter);
  if (parameter.default.kind === 'empty') {
    if (setting.kind === 'list') {
      return setting.items
        .map((item) => settingText(catalogue, parameter.kind, item, items, parameter))
        .join(', ');
    }
    if (setting.kind === 'pairs') return `**${text}`;
  }
  return `${parameter.name}=${text}`;
}

/**
 * Returns how each parameter of `entry` that `options` sets is written in a
 * call: only those that are given everything they need and differ from their
 * default. `items` are what a parameter that follows another is aligned with.
 */
export function optionTexts(
  catalogue: Catalogue,
  entry: EntryView | null,
  options: Options,
  items: (of: string) => readonly string[],
): string[] {
  if (entry === null) return [];
  return setParameters(entry, options)
    .map(({ parameter, setting }) => ({
      parameter,
      setting,
      items: parameter.shape.shape === 'perItem' ? items(parameter.shape.of) : [],
    }))
    .filter(
      ({ parameter, setting, items: each }) =>
        isComplete(setting) && !atDefaultOver(parameter, setting, each),
    )
    .map(({ parameter, setting, items: each }) =>
      parameterText(catalogue, parameter, setting, each),
    );
}

/** Returns the Polars text of one method call, with the parameters it is set with. */
function methodCall(catalogue: Catalogue, method: Method): string {
  const settings = optionTexts(
    catalogue,
    methodEntry(catalogue, method.name),
    method.options,
    () => [],
  );
  return methodText(method, settings);
}

/** Returns why one parameter cannot be worked out as `setting` sets it, or `null` when it can. */
function parameterProblem(
  catalogue: Catalogue,
  parameter: ParameterView,
  setting: Setting,
  frame: Frame,
): string | null {
  if (!isComplete(setting)) return `Give ${parameter.shown} a value (${parameter.label}).`;
  switch (setting.kind) {
    case 'expression':
      return checkValue(catalogue, setting.value, frame).problem;
    case 'word':
      if (parameter.kind.kind === 'column' && findColumn(frame, setting.value) === null) {
        return `There is no column "${setting.value}".`;
      }
      return parameter.kind.kind === 'table' && findInput(setting.value)?.kind !== 'table'
        ? `There is no table "${setting.value}".`
        : null;
    case 'list':
      for (const item of setting.items) {
        const problem = parameterProblem(catalogue, parameter, item, frame);
        if (problem !== null) return problem;
      }
      return null;
    case 'pairs': {
      if (parameter.kind.kind !== 'pairs') return null;
      const { key, value } = parameter.kind;
      for (const entry of setting.entries) {
        const problem =
          parameterProblem(catalogue, { ...parameter, kind: key }, entry.key, frame) ??
          // A field of one side that is another field's new name need not be a field of the rows.
          (value.kind === 'column'
            ? null
            : parameterProblem(catalogue, { ...parameter, kind: value }, entry.value, frame));
        if (problem !== null) return problem;
      }
      return null;
    }
    default:
      return null;
  }
}

/**
 * Returns why the parameters `options` sets for `entry` cannot be worked
 * out, or `null` when they can.
 */
export function optionsProblem(
  catalogue: Catalogue,
  entry: EntryView | null,
  options: Options,
  frame: Frame,
): string | null {
  if (entry === null) return null;
  for (const { parameter, setting } of setParameters(entry, options)) {
    const problem = parameterProblem(catalogue, parameter, setting, frame);
    if (problem !== null) return problem;
  }
  return null;
}
