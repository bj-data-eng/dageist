/**
 * @file The step list in Polars' own vocabulary: `with_columns`, `sort` and
 * `filter`, each working on the frame the step before it produced.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

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
import type { Condition, Expr } from './source';
import {
  checkChain,
  conditionProblem,
  conditionRows,
  conditionText,
  evaluateChain,
  exprText,
  literalText,
} from './source';
import { TABLES, findTable } from './tables';

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'sort' | 'filter' | 'join' | 'pivot';

/** Every verb, in the order it is offered. */
export const VERBS: readonly Verb[] = ['with_columns', 'sort', 'filter', 'join', 'pivot'];

/** `.with_columns(expr)`: adds a column, or replaces one of the same name. */
export interface WithColumnsStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'with_columns';
  readonly expr: Expr;
}

/** `.sort(by…, descending=…)`. */
export interface SortStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'sort';
  readonly by: readonly string[];
  readonly descending: boolean;
}

/** `.filter(condition)`: keeps the rows where the condition is true. */
export interface FilterStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'filter';
  readonly condition: Condition;
}

/** Which rows a join keeps, as Polars names it. */
export type How = 'left' | 'inner';

/** Every way of joining, in the order it is offered. */
export const HOWS: readonly How[] = ['left', 'inner'];

/** `.join(table, left_on=…, right_on=…, how=…)`: brings in another table's columns by a key. */
export interface JoinStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'join';
  /** The name of the other table. */
  readonly table: string;
  readonly leftOn: string;
  readonly rightOn: string;
  readonly how: How;
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
}

/** One step of the list. */
export type Step = WithColumnsStep | SortStep | FilterStep | JoinStep | PivotStep;

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

function withColumns(frame: Frame, step: WithColumnsStep): StepResult {
  const { expr } = step;
  if (expr.alias.trim() === '') return failed(frame, 'alias needs a name.');
  const check = checkChain(expr, frame);
  const cells = evaluateChain(expr, frame);
  const dtype = check.dtypes.at(-1);
  if (check.problem !== null || cells === null || dtype === undefined) {
    return failed(frame, check.problem?.message ?? 'This expression cannot be worked out.');
  }
  const column: Column = { name: expr.alias, dtype, cells };
  const replaced = frame.some((existing) => existing.name === expr.alias);
  return {
    frame: replaced
      ? frame.map((existing) => (existing.name === expr.alias ? column : existing))
      : [...frame, column],
    problem: null,
    added: [expr.alias],
  };
}

function sort(frame: Frame, step: SortStep): StepResult {
  const keys: Column[] = [];
  for (const name of step.by) {
    const column = findColumn(frame, name);
    if (column === null) return failed(frame, `There is no column "${name}".`);
    keys.push(column);
  }
  if (keys.length === 0) return failed(frame, 'sort needs a column.');
  const order = Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
  order.sort((left, right) => {
    for (const key of keys) {
      const first = key.cells[left] ?? null;
      const second = key.cells[right] ?? null;
      const compared = compareCells(first, second);
      if (compared === 0) continue;
      // Polars keeps nulls first whichever way the values are ordered.
      const reverses = step.descending && first !== null && second !== null;
      return reverses ? -compared : compared;
    }
    return left - right;
  });
  return { frame: reordered(frame, order), problem: null, added: [] };
}

function filter(frame: Frame, step: FilterStep): StepResult {
  const problem = conditionProblem(step.condition, frame);
  if (problem !== null) return failed(frame, problem);
  const kept = conditionRows(step.condition, frame).flatMap((holds, row) => (holds ? [row] : []));
  return { frame: reordered(frame, kept), problem: null, added: [] };
}

/** Returns the name a joined-in column gets: its own, or with `_right` when the frame has it already. */
function joinedName(frame: Frame, name: string): string {
  return findColumn(frame, name) === null ? name : `${name}_right`;
}

function join(frame: Frame, step: JoinStep): StepResult {
  const table = findTable(step.table);
  if (table === null) return failed(frame, `There is no table "${step.table}".`);
  const leftKey = findColumn(frame, step.leftOn);
  if (leftKey === null) return failed(frame, `There is no column "${step.leftOn}".`);
  const rightKey = findColumn(table.frame, step.rightOn);
  if (rightKey === null) {
    return failed(frame, `"${step.table}" has no column "${step.rightOn}".`);
  }
  if (!keysFit(leftKey.dtype, rightKey.dtype)) {
    return failed(
      frame,
      `"${leftKey.name}" is ${leftKey.dtype}, but "${step.table}.${rightKey.name}" is ${rightKey.dtype}.`,
    );
  }
  // Each left row is paired with every right row whose key matches. A left
  // join keeps a left row with no match, paired with nulls.
  const pairs: Array<readonly [number, number | null]> = [];
  for (const [row, key] of leftKey.cells.entries()) {
    const matches = rightKey.cells.flatMap((other, at) =>
      key !== null && other === key ? [at] : [],
    );
    if (matches.length === 0 && step.how === 'left') pairs.push([row, null]);
    for (const match of matches) pairs.push([row, match]);
  }
  const kept = reordered(
    frame,
    pairs.map(([row]) => row),
  );
  // Polars keeps one key column, so the right table's key is not brought in.
  const brought: Column[] = table.frame
    .filter((column) => column.name !== step.rightOn)
    .map((column) => ({
      name: joinedName(frame, column.name),
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

function runStep(frame: Frame, step: Step): StepResult {
  switch (step.verb) {
    case 'with_columns':
      return withColumns(frame, step);
    case 'sort':
      return sort(frame, step);
    case 'filter':
      return filter(frame, step);
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
export function runSteps(steps: readonly Step[]): readonly StepResult[] {
  const results: StepResult[] = [];
  let frame = SOURCE_FRAME;
  for (const step of steps) {
    const result = runStep(frame, step);
    results.push(result);
    frame = result.frame;
  }
  return results;
}

/** Returns a new step of `verb` that works on `frame` as it stands. */
export function defaultStep(verb: Verb, frame: Frame): Step {
  const first = frame[0]?.name ?? '';
  const id = nextId('step');
  switch (verb) {
    case 'with_columns':
      return {
        id,
        verb,
        expr: { source: { kind: 'column', column: first }, methods: [], alias: 'new_column' },
      };
    case 'sort':
      return { id, verb, by: [first], descending: false };
    case 'filter':
      return {
        id,
        verb,
        condition: {
          column: first,
          comparison: '==',
          value: { kind: 'literal', value: distinctValues(frame, first)[0] ?? null },
        },
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
        leftOn: leftOn?.name ?? first,
        rightOn: rightOn?.name ?? '',
        how: 'left',
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
      };
    }
    default:
      return assertNever(verb);
  }
}

/** Returns the Polars text of a pivot, with whatever has to come before it. */
function pivotText(step: PivotStep): string {
  const settings = `index=${quoted(step.index)}, values=${quoted(step.values)}, aggregate_function=${quoted(step.aggregate)}`;
  switch (step.columnsFrom) {
    case 'pinned': {
      const columns = step.pinned.map((value) => literalText(value)).join(', ');
      return `.pivot(${quoted(step.on)}, on_columns=[${columns}], ${settings})`;
    }
    case 'collect':
      return `.collect().pivot(${quoted(step.on)}, ${settings}).lazy()`;
    case 'sink':
      return `.sink_parquet("scratch.parquet"); pl.read_parquet("scratch.parquet").pivot(${quoted(step.on)}, ${settings}).lazy()`;
    default:
      return assertNever(step.columnsFrom);
  }
}

/** Returns the Polars method call `step` stands for. */
export function stepText(step: Step): string {
  switch (step.verb) {
    case 'with_columns':
      return `.with_columns(${exprText(step.expr)})`;
    case 'sort': {
      const columns = step.by.map((name) => quoted(name)).join(', ');
      return step.descending ? `.sort(${columns}, descending=True)` : `.sort(${columns})`;
    }
    case 'filter':
      return `.filter(${conditionText(step.condition)})`;
    case 'join':
      return `.join(${step.table}, left_on=${quoted(step.leftOn)}, right_on=${quoted(step.rightOn)}, how=${quoted(step.how)})`;
    case 'pivot':
      return pivotText(step);
    default:
      return assertNever(step);
  }
}
