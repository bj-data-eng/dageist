/**
 * @file The step list in Polars' own vocabulary: `with_columns`, `sort` and
 * `filter`, each working on the frame the step before it produced.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Expr } from './expr';
import { checkExpr, evaluateExpr, exprText } from './expr';
import type { Column, Frame } from './frame';
import {
  SOURCE_FRAME,
  assertNever,
  compareCells,
  distinctValues,
  findColumn,
  reordered,
} from './frame';
import { nextId } from './ids';
import type { Condition } from './source';
import { conditionHolds, conditionProblem, conditionText, quoted } from './source';
import { TABLES, findTable } from './tables';

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'sort' | 'filter' | 'join';

/** Every verb, in the order it is offered. */
export const VERBS: readonly Verb[] = ['with_columns', 'sort', 'filter', 'join'];

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

/** One step of the list. */
export type Step = WithColumnsStep | SortStep | FilterStep | JoinStep;

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
  const check = checkExpr(expr, frame);
  const cells = evaluateExpr(expr, frame);
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
  const rows = Array.from({ length: frame[0]?.cells.length ?? 0 }, (_unused, row) => row);
  const kept = rows.filter((row) => conditionHolds(step.condition, frame, row));
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
  if (leftKey.dtype !== rightKey.dtype) {
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
  return {
    frame: [...kept, ...brought],
    problem: null,
    added: brought.map((column) => column.name),
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
    default:
      return assertNever(verb);
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
    default:
      return assertNever(step);
  }
}
