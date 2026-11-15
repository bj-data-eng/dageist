/**
 * @file The step list in Polars' own vocabulary: `with_columns`, `filter`,
 * `sort`, `select`, `rename`, `join` and `pivot`, each working on the frame
 * the step before it produced.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { ParameterView } from '@dageist/host';

import type { Cell, Column, Frame } from './frame';
import {
  SOURCE_FRAME,
  assertNever,
  compareCells,
  distinctValues,
  findColumn,
  isNumeric,
  keysFit,
  quoted,
  reordered,
} from './frame';
import { nextId } from './ids';
import type { Catalogue } from './catalogue';
import { verbEntry } from './catalogue';
import type { Options } from './settings';
import {
  NO_OPTIONS,
  atDefault,
  flagFor,
  isComplete,
  optionParameters,
  setParameters,
  wordOf,
} from './settings';
import type { Condition, Value } from './source';
import {
  cellsOf,
  checkValue,
  conditionProblem,
  conditionRows,
  conditionText,
  literalText,
  optionTexts,
  optionsProblem,
  valueExprText,
} from './source';
import { TABLES, findTable } from './tables';

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'filter' | 'sort' | 'select' | 'rename' | 'join' | 'pivot';

/** Every verb, in the order it is offered. */
export const VERBS: readonly Verb[] = [
  'with_columns',
  'filter',
  'sort',
  'select',
  'rename',
  'join',
  'pivot',
];

/** `.with_columns(value.alias(alias))`: adds a column, or replaces one of the same name. */
export interface WithColumnsStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'with_columns';
  /** The name of the column. */
  readonly alias: string;
  /** What the column holds: a field, a value written out, an input, or a chain. */
  readonly value: Value;
  /** The other parameters of `with_columns` that are set. */
  readonly options: Options;
}

/**
 * `.sort(by…, descending=[…], nulls_last=[…])`: orders the rows by one column
 * after another. Which way each is ordered, and where the empty values go,
 * are parameters that are set for each column in `options`.
 */
export interface SortStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'sort';
  /** The columns the rows are ordered by. */
  readonly by: readonly string[];
  readonly options: Options;
}

/** `.select(columns…)`: keeps some of the columns, in the order given. */
export interface SelectStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'select';
  readonly columns: readonly string[];
  readonly options: Options;
}

/** One column a rename gives a new name. */
export interface Renaming {
  readonly from: string;
  readonly to: string;
}

/** `.rename({from: to, …})`: gives columns new names. */
export interface RenameStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'rename';
  readonly names: readonly Renaming[];
  readonly options: Options;
}

/** `.filter(condition)`: keeps the rows where the condition is true. */
export interface FilterStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'filter';
  readonly condition: Condition;
  readonly options: Options;
}

/**
 * `.join(table, left_on=…, right_on=…, how=…)`: brings in another table's
 * columns by their keys. Which rows it keeps, `how`, and the other
 * parameters that are set are in `options`.
 */
export interface JoinStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'join';
  /** The name of the other table. */
  readonly table: string;
  /** The columns of these rows that are matched, each with the one at its place in `rightOn`. */
  readonly leftOn: readonly string[];
  /** The columns of the other table that are matched. */
  readonly rightOn: readonly string[];
  readonly options: Options;
}

/** How a pivot combines the values that land in one cell, as Polars names it. */
export type Aggregate = 'first' | 'sum' | 'max' | 'min' | 'len';

/** Every aggregate, in the order it is offered. */
export const AGGREGATES: readonly Aggregate[] = ['first', 'sum', 'max', 'min', 'len'];

/**
 * How a pivot finds out which columns it returns. They are the values of its
 * `on` column, so Polars can only plan the step lazily when they are given up
 * front. Otherwise the rows before it must exist first: loaded into memory
 * with a collect, or written out by a sink.
 */
export type ColumnsFrom = 'collect' | 'sink' | 'pinned';

/** Every way of finding a pivot's columns, in the order it is offered. */
export const COLUMNS_FROM: readonly ColumnsFrom[] = ['collect', 'sink', 'pinned'];

/** `.pivot(on, index=…, values=…, aggregate_function=…)`: turns the values of one column into columns. */
export interface PivotStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'pivot';
  readonly on: string;
  readonly index: string;
  readonly values: string;
  readonly aggregate: Aggregate;
  readonly columnsFrom: ColumnsFrom;
  /** The values of `on` that become columns when the columns are pinned. */
  readonly pinned: readonly Cell[];
  readonly options: Options;
}

/** One step of the list. */
export type Step =
  | WithColumnsStep
  | FilterStep
  | SortStep
  | SelectStep
  | RenameStep
  | JoinStep
  | PivotStep;

/** Where the lazy query has to stop before `step` can run, if it has to. */
export function stepBreak(step: Step): 'collect' | 'sink' | null {
  return step.verb === 'pivot' && step.columnsFrom !== 'pinned' ? step.columnsFrom : null;
}

/** What a step produced: its frame, or the frame it was given and why it failed. */
export interface StepResult {
  readonly frame: Frame;
  readonly problem: string | null;
  /** The columns this step added or replaced. */
  readonly added: readonly string[];
}

function failed(frame: Frame, problem: string): StepResult {
  return { frame, problem, added: [] };
}

function withColumns(catalogue: Catalogue, frame: Frame, step: WithColumnsStep): StepResult {
  const { alias, value } = step;
  if (alias.trim() === '') return failed(frame, 'alias needs a name.');
  const check = checkValue(catalogue, value, frame);
  if (check.problem !== null) return failed(frame, check.problem);
  if (check.dtype === null) return failed(frame, 'None alone has no dtype to make a column of.');
  const column: Column = {
    name: alias,
    dtype: check.dtype,
    cells: cellsOf(catalogue, value, frame),
  };
  const replaced = frame.some((existing) => existing.name === alias);
  return {
    frame: replaced
      ? frame.map((existing) => (existing.name === alias ? column : existing))
      : [...frame, column],
    problem: null,
    added: [alias],
  };
}

function sort(frame: Frame, step: SortStep): StepResult {
  const keys: Array<{
    readonly cells: readonly Cell[];
    readonly descending: boolean;
    readonly nullsLast: boolean;
  }> = [];
  for (const name of step.by) {
    const column = findColumn(frame, name);
    if (column === null) return failed(frame, `There is no column "${name}".`);
    keys.push({
      cells: column.cells,
      descending: flagFor(step.options, 'descending', name),
      nullsLast: flagFor(step.options, 'nulls_last', name),
    });
  }
  if (keys.length === 0) return failed(frame, 'Pick the fields to sort by.');
  const order = Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
  order.sort((left, right) => {
    for (const key of keys) {
      const first = key.cells[left] ?? null;
      const second = key.cells[right] ?? null;
      if (first === null || second === null) {
        // Empty values go first, or last when the column says so, whichever way the values are ordered.
        if (first === second) continue;
        return (first === null) === key.nullsLast ? 1 : -1;
      }
      const compared = compareCells(first, second);
      if (compared === 0) continue;
      return key.descending ? -compared : compared;
    }
    return left - right;
  });
  return { frame: reordered(frame, order), problem: null, added: [] };
}

function select(frame: Frame, step: SelectStep): StepResult {
  if (step.columns.length === 0) return failed(frame, 'Pick the fields to keep.');
  // Polars refuses to select one column twice.
  const twice = step.columns.find((name, at) => step.columns.indexOf(name) !== at);
  if (twice !== undefined) return failed(frame, `"${twice}" is selected twice.`);
  const kept: Column[] = [];
  for (const name of step.columns) {
    const column = findColumn(frame, name);
    if (column === null) return failed(frame, `There is no column "${name}".`);
    kept.push(column);
  }
  return { frame: kept, problem: null, added: [] };
}

function rename(frame: Frame, step: RenameStep): StepResult {
  const renamed = new Map<string, string>();
  for (const { from, to } of step.names) {
    if (findColumn(frame, from) === null) return failed(frame, `There is no column "${from}".`);
    if (to === '') return failed(frame, `"${from}" needs a new name.`);
    renamed.set(from, to);
  }
  const next = frame.map((column) => ({
    ...column,
    name: renamed.get(column.name) ?? column.name,
  }));
  // Polars refuses a rename that would leave two columns with one name.
  const taken = new Set<string>();
  for (const column of next) {
    if (taken.has(column.name)) {
      return failed(frame, `The rename would leave two columns called "${column.name}".`);
    }
    taken.add(column.name);
  }
  return { frame: next, problem: null, added: [...renamed.values()] };
}

function filter(catalogue: Catalogue, frame: Frame, step: FilterStep): StepResult {
  const problem = conditionProblem(catalogue, step.condition, frame);
  if (problem !== null) return failed(frame, problem);
  const kept = conditionRows(catalogue, step.condition, frame).flatMap((holds, row) =>
    holds ? [row] : [],
  );
  return { frame: reordered(frame, kept), problem: null, added: [] };
}

/** How a join names a column it brings in when the frame has one of that name already. */
const DEFAULT_SUFFIX = '_right';

/** Returns the name a joined-in column gets: its own, or with `suffix` when the frame has it already. */
function joinedName(frame: Frame, name: string, suffix: string): string {
  return findColumn(frame, name) === null ? name : `${name}${suffix}`;
}

function join(frame: Frame, step: JoinStep): StepResult {
  const table = findTable(step.table);
  if (table === null) return failed(frame, `There is no table "${step.table}".`);
  if (step.leftOn.length === 0) return failed(frame, 'Pick the fields to match.');
  if (step.leftOn.length !== step.rightOn.length) {
    return failed(
      frame,
      `${String(step.leftOn.length)} fields of these rows are matched with ${String(step.rightOn.length)} of "${step.table}".`,
    );
  }
  const keys: Array<{ readonly left: Column; readonly right: Column }> = [];
  for (const [at, leftName] of step.leftOn.entries()) {
    const rightName = step.rightOn[at] ?? '';
    const left = findColumn(frame, leftName);
    if (left === null) return failed(frame, `There is no column "${leftName}".`);
    const right = findColumn(table.frame, rightName);
    if (right === null) return failed(frame, `"${step.table}" has no column "${rightName}".`);
    if (!keysFit(left.dtype, right.dtype)) {
      return failed(
        frame,
        `"${left.name}" is ${left.dtype}, but "${step.table}.${right.name}" is ${right.dtype}.`,
      );
    }
    keys.push({ left, right });
  }
  // The preview keeps the rows of the joins it knows; any other kind is said not to be previewed.
  const how = wordOf(step.options, 'how') ?? 'inner';
  if (how !== 'left' && how !== 'inner') {
    return failed(frame, `The preview does not include how="${how}".`);
  }
  const suffix = wordOf(step.options, 'suffix') ?? DEFAULT_SUFFIX;
  // Each left row is paired with every right row whose keys all match. A left
  // join keeps a left row with no match, paired with nulls.
  const pairs: Array<readonly [number, number | null]> = [];
  const rows = frame[0]?.cells.length ?? 0;
  for (let row = 0; row < rows; row += 1) {
    const matches = (table.frame[0]?.cells ?? []).flatMap((_cell, at) =>
      keys.every(({ left, right }) => {
        const key = left.cells[row] ?? null;
        return key !== null && right.cells[at] === key;
      })
        ? [at]
        : [],
    );
    if (matches.length === 0 && how === 'left') pairs.push([row, null]);
    for (const match of matches) pairs.push([row, match]);
  }
  const kept = reordered(
    frame,
    pairs.map(([row]) => row),
  );
  // Polars keeps the left key columns, so the right table's keys are not brought in.
  const brought: Column[] = table.frame
    .filter((column) => !step.rightOn.includes(column.name))
    .map((column) => ({
      name: joinedName(frame, column.name, suffix),
      dtype: column.dtype,
      cells: pairs.map(([, match]) => (match === null ? null : (column.cells[match] ?? null))),
    }));
  // Polars refuses a join that would leave two columns with one name.
  const taken = new Set(frame.map((column) => column.name));
  for (const column of brought) {
    if (taken.has(column.name)) {
      return failed(frame, `The join would add a second column "${column.name}".`);
    }
    taken.add(column.name);
  }
  return {
    frame: [...kept, ...brought],
    problem: null,
    added: brought.map((column) => column.name),
  };
}

/**
 * Returns what `aggregate` gives the cells that land in one pivot cell. Polars
 * works each cell out as that aggregate over the rows that match, so a cell
 * no row lands in holds what the aggregate gives no rows: 0 for `len` and
 * `sum`, and null for the others.
 */
function aggregated(aggregate: Aggregate, cells: readonly Cell[]): Cell {
  const present = cells.filter((cell) => cell !== null);
  const [first] = present;
  switch (aggregate) {
    case 'len':
      return cells.length;
    case 'first':
      // The first row's value, null or not.
      return cells[0] ?? null;
    case 'sum':
      return present.reduce<number>(
        (total, cell) => total + (typeof cell === 'number' ? cell : 0),
        0,
      );
    case 'max':
      if (first === undefined) return null;
      return present.reduce((best, cell) => (compareCells(cell, best) > 0 ? cell : best), first);
    case 'min':
      if (first === undefined) return null;
      return present.reduce((best, cell) => (compareCells(cell, best) < 0 ? cell : best), first);
    default:
      return assertNever(aggregate);
  }
}

function pivot(frame: Frame, step: PivotStep): StepResult {
  const on = findColumn(frame, step.on);
  if (on === null) return failed(frame, `There is no column "${step.on}".`);
  const index = findColumn(frame, step.index);
  if (index === null) return failed(frame, `There is no column "${step.index}".`);
  const values = findColumn(frame, step.values);
  if (values === null) return failed(frame, `There is no column "${step.values}".`);
  if (step.aggregate === 'sum' && !isNumeric(values.dtype) && values.dtype !== 'Duration') {
    return failed(
      frame,
      `sum needs a numeric dtype or Duration, but "${values.name}" is ${values.dtype}.`,
    );
  }
  const headers = step.columnsFrom === 'pinned' ? step.pinned : distinctValues(frame, step.on);
  const keys = distinctValues(frame, step.index);
  const dtype = step.aggregate === 'len' ? 'UInt32' : values.dtype;
  const pivoted: Column[] = headers.map((header) => ({
    name: String(header),
    dtype,
    cells: keys.map((key) =>
      aggregated(
        step.aggregate,
        values.cells.filter((_cell, row) => on.cells[row] === header && index.cells[row] === key),
      ),
    ),
  }));
  return {
    frame: [{ name: index.name, dtype: index.dtype, cells: keys }, ...pivoted],
    problem: null,
    added: pivoted.map((column) => column.name),
  };
}

/** The parameters each step's own structure draws, which are not offered as options. */
const COVERED: Readonly<Record<Verb, readonly string[]>> = {
  // A column and what it holds are the step's own structure.
  with_columns: ['exprs', 'named_exprs'],
  filter: ['predicates'],
  // The fields to order by are one list, so a second one for more fields is not offered.
  sort: ['by', 'more_by'],
  select: ['exprs'],
  rename: ['mapping'],
  // The keys of the two tables are one list each, so `on` for a shared key is not offered.
  join: ['other', 'on', 'left_on', 'right_on'],
  pivot: ['on', 'on_columns', 'index', 'values', 'aggregate_function'],
};

/** Returns the parameters of the step `verb` that can be added to it as options. */
export function stepOptionParameters(catalogue: Catalogue, verb: Verb): readonly ParameterView[] {
  const entry = verbEntry(catalogue, verb);
  return entry === null ? [] : optionParameters(entry, COVERED[verb]);
}

/** Returns the items a parameter of `step` that follows the parameter `of` holds one value for each of. */
export function stepItems(step: Step, of: string): readonly string[] {
  return step.verb === 'sort' && of === 'by' ? step.by : [];
}

/** The parameters of each step the preview applies when they are set. */
const PREVIEWED_OPTIONS: Readonly<Record<Verb, readonly string[]>> = {
  with_columns: [],
  filter: [],
  sort: ['descending', 'nulls_last'],
  select: [],
  rename: [],
  join: ['how', 'suffix'],
  pivot: [],
};

/**
 * Returns the Polars names of the parameters `step` is set with that the
 * preview does not apply, so that the step can say so.
 */
export function skippedStepOptions(catalogue: Catalogue, step: Step): string[] {
  const entry = verbEntry(catalogue, step.verb);
  if (entry === null) return [];
  const applied = PREVIEWED_OPTIONS[step.verb];
  return setParameters(entry, step.options)
    .filter(
      ({ parameter, setting }) =>
        isComplete(setting) && !atDefault(parameter, setting) && !applied.includes(parameter.name),
    )
    .map(({ parameter }) => parameter.name);
}

function runStep(catalogue: Catalogue, frame: Frame, step: Step): StepResult {
  const problem = optionsProblem(catalogue, verbEntry(catalogue, step.verb), step.options, frame);
  if (problem !== null) return failed(frame, problem);
  switch (step.verb) {
    case 'with_columns':
      return withColumns(catalogue, frame, step);
    case 'sort':
      return sort(frame, step);
    case 'filter':
      return filter(catalogue, frame, step);
    case 'select':
      return select(frame, step);
    case 'rename':
      return rename(frame, step);
    case 'join':
      return join(frame, step);
    case 'pivot':
      return pivot(frame, step);
    default:
      return assertNever(step);
  }
}

/**
 * Works every step out in order, starting from the source frame. A step with
 * a problem passes its frame on unchanged, so later steps still show something.
 */
export function runSteps(catalogue: Catalogue, steps: readonly Step[]): readonly StepResult[] {
  const results: StepResult[] = [];
  let frame = SOURCE_FRAME;
  for (const step of steps) {
    const result = runStep(catalogue, frame, step);
    results.push(result);
    frame = result.frame;
  }
  return results;
}

/** Returns a new step of `verb` that works on `frame` as it stands. */
export function defaultStep(verb: Verb, frame: Frame): Step {
  const first = frame[0]?.name ?? '';
  const id = nextId('step');
  const options = NO_OPTIONS;
  switch (verb) {
    case 'with_columns':
      return {
        id,
        verb,
        alias: 'new_column',
        // What the column holds is still to be picked.
        value: { kind: 'unset' },
        options,
      };
    case 'sort':
      return { id, verb, by: [], options };
    case 'select':
      return { id, verb, columns: frame.map((column) => column.name), options };
    case 'rename':
      return { id, verb, names: [], options };
    case 'filter':
      return {
        id,
        verb,
        condition: {
          // A comparison whose field and value are still to be picked.
          kind: 'test',
          id: nextId('test'),
          column: '',
          comparison: '==',
          value: { kind: 'unset' },
        },
        options,
      };
    case 'join': {
      // Start from the first lookup table and a pair of key columns whose dtypes agree.
      const [table] = TABLES;
      const rightOn = table?.frame[0];
      const leftOn = frame.find((column) => column.dtype === rightOn?.dtype);
      return {
        id,
        verb,
        table: table?.name ?? '',
        leftOn: [leftOn?.name ?? first],
        rightOn: [rightOn?.name ?? ''],
        // Rows with no match are kept, which is not what Polars does unless it is asked.
        options: { how: { kind: 'word', value: 'left' } },
      };
    }
    case 'pivot': {
      const text = frame.filter((column) => column.dtype === 'String');
      const number = frame.find((column) => isNumeric(column.dtype));
      return {
        id,
        verb,
        on: (text[1] ?? text[0])?.name ?? first,
        index: first,
        values: number?.name ?? first,
        aggregate: 'first',
        columnsFrom: 'collect',
        pinned: [],
        options,
      };
    }
    default:
      return assertNever(verb);
  }
}

/** Returns `args` as the arguments of a call, then the text of the parameters that are set. */
function callText(args: readonly string[], settings: readonly string[]): string {
  return [...args, ...settings].join(', ');
}

/** Returns the Polars text of a pivot, with whatever has to come before it. */
function pivotText(step: PivotStep, settings: readonly string[]): string {
  const given = `index=${quoted(step.index)}, values=${quoted(step.values)}, aggregate_function=${quoted(step.aggregate)}`;
  const rest = callText([given], settings);
  switch (step.columnsFrom) {
    case 'pinned': {
      const columns = step.pinned.map((value) => literalText(value)).join(', ');
      return `.pivot(${quoted(step.on)}, on_columns=[${columns}], ${rest})`;
    }
    case 'collect':
      return `.collect().pivot(${quoted(step.on)}, ${rest}).lazy()`;
    case 'sink':
      return `.sink_parquet("scratch.parquet"); pl.read_parquet("scratch.parquet").pivot(${quoted(step.on)}, ${rest}).lazy()`;
    default:
      return assertNever(step.columnsFrom);
  }
}

/** Returns `names` as the argument of a call: the one name, or a list of them. */
function namesText(names: readonly string[]): string {
  const [only] = names;
  return names.length === 1 && only !== undefined
    ? quoted(only)
    : `[${names.map((name) => quoted(name)).join(', ')}]`;
}

/** Returns the Polars method call `step` stands for. */
export function stepText(catalogue: Catalogue, step: Step): string {
  // Only the parameters that are set and differ from their default are written.
  const settings = optionTexts(catalogue, verbEntry(catalogue, step.verb), step.options, (of) =>
    stepItems(step, of),
  );
  switch (step.verb) {
    case 'with_columns':
      return `.with_columns(${callText([`${valueExprText(catalogue, step.value)}.alias(${quoted(step.alias)})`], settings)})`;
    case 'sort':
      return `.sort(${callText(
        step.by.map((name) => quoted(name)),
        settings,
      )})`;
    case 'select':
      return `.select(${callText(
        step.columns.map((name) => quoted(name)),
        settings,
      )})`;
    case 'rename': {
      const names = step.names.map(({ from, to }) => `${quoted(from)}: ${quoted(to)}`);
      return `.rename(${callText([`{${names.join(', ')}}`], settings)})`;
    }
    case 'filter':
      return `.filter(${callText([conditionText(catalogue, step.condition)], settings)})`;
    case 'join':
      return `.join(${callText([step.table, `left_on=${namesText(step.leftOn)}`, `right_on=${namesText(step.rightOn)}`], settings)})`;
    case 'pivot':
      return pivotText(step, settings);
    default:
      return assertNever(step);
  }
}
