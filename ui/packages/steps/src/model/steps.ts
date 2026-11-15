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
import type { MethodName } from './expr';
import { nextId } from './ids';
import type { Catalogue } from './catalogue';
import { verbEntry } from './catalogue';
import type { Options, Setting } from './settings';
import {
  NO_OPTIONS,
  UNSET,
  atDefault,
  defaultSetting,
  expressionSetting,
  flagFor,
  isComplete,
  itemsOf,
  optionOf,
  optionParameters,
  pairsOf,
  setParameters,
  textOf,
  withoutOptions,
  wordOf,
} from './settings';
import type { Chain, Condition, Value } from './source';
import {
  cellsOf,
  checkChain,
  checkValue,
  conditionProblem,
  conditionRows,
  conditionText,
  evaluateElement,
  keyText,
  literalText,
  optionTexts,
  optionsProblem,
  settingValueText,
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
 * `.sort(by…, descending=[…], nulls_last=[…])`: orders the rows by one key
 * after another. The keys are the list `by` of `options`, each an expression.
 * Which way each is ordered, and where the empty values go, are parameters
 * that are set for each key in `options` too.
 */
export interface SortStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'sort';
  readonly options: Options;
}

/**
 * `.select(exprs…)`: keeps some of the columns, in the order given. The
 * expressions are the list `exprs` of `options`.
 */
export interface SelectStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'select';
  readonly options: Options;
}

/**
 * `.rename({from: to, …})`: gives columns new names. The pairs are the
 * parameter `mapping` of `options`.
 */
export interface RenameStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'rename';
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
 * columns by their keys. The keys are the lists `left_on` and `right_on` of
 * `options`, each key an expression, matched with the one at its place on the
 * other side. Which rows it keeps, `how`, and the other parameters that are
 * set are in `options` too.
 */
export interface JoinStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'join';
  /** The name of the other table. */
  readonly table: string;
  readonly options: Options;
}

/**
 * How a pivot finds out which columns it returns. They are the values of its
 * `on` column, so Polars can only plan the step lazily when they are given up
 * front. Otherwise the rows before it must exist first: loaded into memory
 * with a collect, or written out by a sink.
 */
export type ColumnsFrom = 'collect' | 'sink' | 'pinned';

/** Every way of finding a pivot's columns, in the order it is offered. */
export const COLUMNS_FROM: readonly ColumnsFrom[] = ['collect', 'sink', 'pinned'];

/**
 * `.pivot(on, index=…, values=…, aggregate_function=…)`: turns the values of
 * one column into columns. How the values that land in one cell are combined
 * is the expression `aggregate_function` of `options`, made of `pl.element()`.
 */
export interface PivotStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'pivot';
  readonly on: string;
  readonly index: string;
  readonly values: string;
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

/** A value that is still to be picked. */
const NOTHING: Value = { kind: 'unset' };

/** Returns the keys of the list parameter `name` of `options`: each item's expression, or nothing yet. */
export function keysOf(options: Options, name: string): readonly Value[] {
  return itemsOf(options, name).map((item) => (item.kind === 'expression' ? item.value : NOTHING));
}

/** Returns a list of keys that are the fields called `names`, as the parameter of a step holds them. */
export function fieldKeys(names: readonly string[]): Setting {
  return {
    kind: 'list',
    items: names.map((column) => expressionSetting({ kind: 'column', column })),
  };
}

/** Returns what `pl.element()` followed by the method `name` gives: the chain an aggregate is made of. */
export function elementChain(name: MethodName): Chain {
  return {
    source: { kind: 'element' },
    methods: [{ id: nextId('method'), name, argument: null, options: NO_OPTIONS }],
  };
}

/** Returns the aggregate of `step`: the expression its values are combined by, when it holds one. */
export function aggregateOf(step: PivotStep): Value | null {
  const setting = optionOf(step.options, 'aggregate_function');
  return setting?.kind === 'expression' ? setting.value : null;
}

/** Returns how `key` is told apart from the other keys of a list: a field by its name, anything else by its text. */
function keyLabel(catalogue: Catalogue, key: Value): string {
  return key.kind === 'column' ? key.column : keyText(catalogue, key);
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

function sort(catalogue: Catalogue, frame: Frame, step: SortStep): StepResult {
  const keys: Array<{
    readonly cells: readonly Cell[];
    readonly descending: boolean;
    readonly nullsLast: boolean;
  }> = [];
  for (const key of keysOf(step.options, 'by')) {
    // The preview orders the rows by fields; any other key it does not work out.
    if (key.kind !== 'column') {
      return failed(
        frame,
        `The preview does not include sorting by ${keyText(catalogue, key)}, only by fields.`,
      );
    }
    const column = findColumn(frame, key.column);
    if (column === null) return failed(frame, `There is no column "${key.column}".`);
    keys.push({
      cells: column.cells,
      descending: flagFor(step.options, 'descending', key.column),
      nullsLast: flagFor(step.options, 'nulls_last', key.column),
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

function select(catalogue: Catalogue, frame: Frame, step: SelectStep): StepResult {
  const names: string[] = [];
  for (const key of keysOf(step.options, 'exprs')) {
    // The preview keeps fields; any other expression it does not work out.
    if (key.kind !== 'column') {
      return failed(
        frame,
        `The preview does not include selecting ${keyText(catalogue, key)}, only fields.`,
      );
    }
    // Polars refuses to select one column twice.
    if (names.includes(key.column)) return failed(frame, `"${key.column}" is selected twice.`);
    names.push(key.column);
  }
  if (names.length === 0) return failed(frame, 'Pick the fields to keep.');
  const kept: Column[] = [];
  for (const name of names) {
    const column = findColumn(frame, name);
    if (column === null) return failed(frame, `There is no column "${name}".`);
    kept.push(column);
  }
  return { frame: kept, problem: null, added: [] };
}

function rename(frame: Frame, step: RenameStep): StepResult {
  const renamed = new Map<string, string>();
  for (const { key, value } of pairsOf(step.options, 'mapping')) {
    const from = textOf(key) ?? '';
    const to = textOf(value) ?? '';
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

/**
 * Returns the name of the field `key` is, or the result that says why the
 * preview cannot work with it. `of` names the table the key is a field of.
 */
function joinField(
  catalogue: Catalogue,
  frame: Frame,
  key: Value,
  table: Frame,
  of: string,
): { readonly column: Column } | StepResult {
  if (key.kind === 'unset') return failed(frame, `Pick the fields of ${of} to match.`);
  if (key.kind !== 'column') {
    const checked = checkValue(catalogue, key, table);
    if (checked.problem !== null) return failed(frame, checked.problem);
    return failed(
      frame,
      `The preview does not include joining on ${keyText(catalogue, key)}, only on fields.`,
    );
  }
  const column = findColumn(table, key.column);
  return column === null ? failed(frame, `No column "${key.column}" in ${of}.`) : { column };
}

function join(catalogue: Catalogue, frame: Frame, step: JoinStep): StepResult {
  const table = findTable(step.table);
  if (table === null) return failed(frame, `There is no table "${step.table}".`);
  const leftOn = keysOf(step.options, 'left_on');
  const rightOn = keysOf(step.options, 'right_on');
  if (leftOn.length === 0) return failed(frame, 'Pick the fields to match.');
  if (leftOn.length !== rightOn.length) {
    return failed(
      frame,
      `${String(leftOn.length)} fields of these rows are matched with ${String(rightOn.length)} of "${step.table}".`,
    );
  }
  const keys: Array<{ readonly left: Column; readonly right: Column }> = [];
  for (const [at, leftKey] of leftOn.entries()) {
    const left = joinField(catalogue, frame, leftKey, frame, 'these rows');
    if (!('column' in left)) return left;
    const right = joinField(
      catalogue,
      frame,
      rightOn[at] ?? NOTHING,
      table.frame,
      `"${step.table}"`,
    );
    if (!('column' in right)) return right;
    if (!keysFit(left.column.dtype, right.column.dtype)) {
      return failed(
        frame,
        `"${left.column.name}" is ${left.column.dtype}, but "${step.table}.${right.column.name}" is ${right.column.dtype}.`,
      );
    }
    keys.push({ left: left.column, right: right.column });
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
    .filter((column) => !keys.some(({ right }) => right.name === column.name))
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

function pivot(catalogue: Catalogue, frame: Frame, step: PivotStep): StepResult {
  const on = findColumn(frame, step.on);
  if (on === null) return failed(frame, `There is no column "${step.on}".`);
  const index = findColumn(frame, step.index);
  if (index === null) return failed(frame, `There is no column "${step.index}".`);
  const values = findColumn(frame, step.values);
  if (values === null) return failed(frame, `There is no column "${step.values}".`);
  // Polars works each cell out as the aggregate over the values that land in it.
  const aggregate = aggregateOf(step);
  if (aggregate === null || aggregate.kind === 'unset') {
    return failed(frame, 'Pick how the values that land in one cell are combined.');
  }
  if (aggregate.kind !== 'chain') {
    return failed(frame, 'The values that land in one cell are combined by a chain of element.');
  }
  const { chain } = aggregate;
  const check = checkChain(catalogue, chain, frame, { dtype: values.dtype });
  if (check.problem !== null) return failed(frame, check.problem.message);
  const dtype = check.dtypes.at(-1) ?? values.dtype;
  const text = valueExprText(catalogue, aggregate);
  // A cell no row lands in holds what the aggregate gives no rows.
  const headers = step.columnsFrom === 'pinned' ? step.pinned : distinctValues(frame, step.on);
  const keys = distinctValues(frame, step.index);
  const pivoted: Column[] = [];
  for (const header of headers) {
    const cells: Cell[] = [];
    for (const key of keys) {
      const landing = values.cells.filter(
        (_cell, row) => on.cells[row] === header && index.cells[row] === key,
      );
      const one = evaluateElement(chain, landing, frame);
      if (one === undefined) {
        return failed(
          frame,
          `The preview does not include ${text}: it does not end in a method that gives one value.`,
        );
      }
      cells.push(one);
    }
    pivoted.push({ name: String(header), dtype, cells });
  }
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
  // The fields to keep are one list, so another expression is not offered by name.
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

/**
 * The parameters of each step that the step itself works out, instead of the
 * generic check of a parameter against the rows it is given.
 */
const CHECKED_BY_STEP: Readonly<Record<Verb, readonly string[]>> = {
  with_columns: [],
  filter: [],
  sort: [],
  select: [],
  rename: [],
  // The right keys are fields of the other table.
  join: ['right_on'],
  // The aggregate is made of the values of a cell, not of the rows.
  pivot: ['aggregate_function'],
};

/**
 * Returns the items a parameter of `step` that follows the parameter `of`
 * holds one value for each of: the keys of a sort that are given, each by the
 * name of its field or else its text.
 */
export function stepItems(catalogue: Catalogue, step: Step, of: string): readonly string[] {
  if (step.verb !== 'sort' || of !== 'by') return [];
  return keysOf(step.options, 'by').flatMap((key) =>
    key.kind === 'unset' ? [] : [keyLabel(catalogue, key)],
  );
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
  const covered = COVERED[step.verb];
  return setParameters(entry, step.options)
    .filter(
      ({ parameter, setting }) =>
        !covered.includes(parameter.name) &&
        isComplete(setting) &&
        !atDefault(parameter, setting) &&
        !applied.includes(parameter.name),
    )
    .map(({ parameter }) => parameter.name);
}

function runStep(catalogue: Catalogue, frame: Frame, step: Step): StepResult {
  const problem = optionsProblem(
    catalogue,
    verbEntry(catalogue, step.verb),
    withoutOptions(step.options, CHECKED_BY_STEP[step.verb]),
    frame,
  );
  if (problem !== null) return failed(frame, problem);
  switch (step.verb) {
    case 'with_columns':
      return withColumns(catalogue, frame, step);
    case 'sort':
      return sort(catalogue, frame, step);
    case 'filter':
      return filter(catalogue, frame, step);
    case 'select':
      return select(catalogue, frame, step);
    case 'rename':
      return rename(frame, step);
    case 'join':
      return join(catalogue, frame, step);
    case 'pivot':
      return pivot(catalogue, frame, step);
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
      // The first key is still to be picked.
      return { id, verb, options: { by: { kind: 'list', items: [UNSET] } } };
    case 'select':
      return {
        id,
        verb,
        options: { exprs: fieldKeys(frame.map((column) => column.name)) },
      };
    case 'rename':
      return { id, verb, options: { mapping: { kind: 'pairs', entries: [] } } };
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
        options: {
          left_on: fieldKeys([leftOn?.name ?? first]),
          right_on: fieldKeys([rightOn?.name ?? '']),
          // Rows with no match are kept, which is not what Polars does unless it is asked.
          how: { kind: 'word', value: 'left' },
        },
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
        columnsFrom: 'collect',
        pinned: [],
        options: {
          aggregate_function: expressionSetting({
            kind: 'chain',
            id: nextId('chain'),
            chain: elementChain('first'),
          }),
        },
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

/**
 * The parameters whose default `DataFrame.pivot`, which the Polars text calls
 * when the columns come from the data, has other than `LazyFrame.pivot`'s,
 * with the default it has: it keeps the order of its rows unless it is told
 * not to.
 */
const EAGER_PIVOT_DEFAULTS: Readonly<Record<string, boolean>> = { maintain_order: true };

/**
 * Returns the text of the parameters of a pivot that are set, as `DataFrame.pivot`
 * is called with them: each whose default there differs from `LazyFrame.pivot`'s
 * is written whenever the editor's value differs from the default there, so
 * that the code does what the editor shows.
 */
function eagerPivotSettings(catalogue: Catalogue, step: PivotStep): string[] {
  const entry = verbEntry(catalogue, step.verb);
  if (entry === null) return [];
  const own = Object.keys(EAGER_PIVOT_DEFAULTS);
  const texts = optionTexts(
    catalogue,
    entry,
    withoutOptions(step.options, [...COVERED.pivot, ...own]),
    () => [],
  );
  const differing = own.flatMap((name) => {
    const parameter = entry.parameters.find((candidate) => candidate.name === name);
    if (parameter === undefined) return [];
    // What the editor shows: what is set, or else what `LazyFrame.pivot` does.
    const set = optionOf(step.options, name);
    const shown = set !== null && isComplete(set) ? set : defaultSetting(parameter);
    const kept = EAGER_PIVOT_DEFAULTS[name];
    if (shown?.kind !== 'flag' || shown.value === kept) return [];
    return [`${name}=${shown.value ? 'True' : 'False'}`];
  });
  return [...texts, ...differing];
}

/** Returns the Polars text of a pivot, with whatever has to come before it. */
function pivotText(catalogue: Catalogue, step: PivotStep, settings: readonly string[]): string {
  const aggregate = optionOf(step.options, 'aggregate_function');
  const entry = verbEntry(catalogue, step.verb);
  const parameter = entry?.parameters.find((candidate) => candidate.name === 'aggregate_function');
  const given = [
    `index=${quoted(step.index)}`,
    `values=${quoted(step.values)}`,
    ...(aggregate === null || parameter === undefined
      ? []
      : [`aggregate_function=${settingValueText(catalogue, parameter, aggregate, [])}`]),
  ];
  switch (step.columnsFrom) {
    case 'pinned': {
      const columns = step.pinned.map((value) => literalText(value)).join(', ');
      return `.pivot(${quoted(step.on)}, on_columns=[${columns}], ${callText(given, settings)})`;
    }
    case 'collect':
      return `.collect().pivot(${quoted(step.on)}, ${callText(given, eagerPivotSettings(catalogue, step))}).lazy()`;
    case 'sink':
      return `.sink_parquet("scratch.parquet"); pl.read_parquet("scratch.parquet").pivot(${quoted(step.on)}, ${callText(given, eagerPivotSettings(catalogue, step))}).lazy()`;
    default:
      return assertNever(step.columnsFrom);
  }
}

/** Returns `texts` as the argument of a call: the one, or a list of them. */
function listText(texts: readonly string[]): string {
  const [only] = texts;
  return texts.length === 1 && only !== undefined ? only : `[${texts.join(', ')}]`;
}

/** Returns the Polars method call `step` stands for. */
export function stepText(catalogue: Catalogue, step: Step): string {
  const entry = verbEntry(catalogue, step.verb);
  // Only the parameters that are set and differ from their default are written,
  // and not those the step writes itself.
  const settings = optionTexts(
    catalogue,
    entry,
    withoutOptions(step.options, COVERED[step.verb]),
    (of) => stepItems(catalogue, step, of),
  );
  switch (step.verb) {
    case 'with_columns':
      return `.with_columns(${callText([`${valueExprText(catalogue, step.value)}.alias(${quoted(step.alias)})`], settings)})`;
    case 'sort':
      return `.sort(${callText(
        keysOf(step.options, 'by').map((key) => keyText(catalogue, key)),
        settings,
      )})`;
    case 'select':
      return `.select(${callText(
        keysOf(step.options, 'exprs').map((key) => keyText(catalogue, key)),
        settings,
      )})`;
    case 'rename': {
      const mapping = entry?.parameters.find((candidate) => candidate.name === 'mapping');
      const setting = optionOf(step.options, 'mapping');
      const text =
        mapping === undefined || setting === null
          ? '{}'
          : settingValueText(catalogue, mapping, setting, []);
      return `.rename(${callText([text], settings)})`;
    }
    case 'filter':
      return `.filter(${callText([conditionText(catalogue, step.condition)], settings)})`;
    case 'join': {
      const keys = (name: string): string =>
        listText(keysOf(step.options, name).map((key) => keyText(catalogue, key)));
      return `.join(${callText([step.table, `left_on=${keys('left_on')}`, `right_on=${keys('right_on')}`], settings)})`;
    }
    case 'pivot':
      return pivotText(catalogue, step, settings);
    default:
      return assertNever(step);
  }
}
