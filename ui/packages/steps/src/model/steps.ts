/**
 * @file The step list in Polars' own vocabulary: `with_columns`, `sort` and
 * `filter`, each working on the frame the step before it produced.
 *
 * This is storyboard code. In the real application the Rust core owns types,
 * validation and previews; nothing here is meant to survive.
 */

import type { Expr } from './expr';
import { checkExpr, evaluateExpr, exprText, quoted } from './expr';
import type { Cell, Column, Frame } from './frame';
import {
  SOURCE_FRAME,
  assertNever,
  compareCells,
  distinctValues,
  findColumn,
  reordered,
} from './frame';
import { nextId } from './ids';

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'sort' | 'filter';

/** Every verb, in the order it is offered. */
export const VERBS: readonly Verb[] = ['with_columns', 'sort', 'filter'];

/** The comparison operators a filter can use. */
export type Comparison = '==' | '!=';

/** Every comparison, in the order it is offered. */
export const COMPARISONS: readonly Comparison[] = ['==', '!='];

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

/** `.filter(pl.col(column) <comparison> value)`. */
export interface FilterStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'filter';
  readonly column: string;
  readonly comparison: Comparison;
  readonly value: Cell;
}

/** One step of the list. */
export type Step = WithColumnsStep | SortStep | FilterStep;

/** What a step produced: its frame, or the frame it was given and why it failed. */
export interface StepResult {
  readonly frame: Frame;
  readonly problem: string | null;
  /** The column this step added or replaced, if it added one. */
  readonly added: string | null;
}

function failed(frame: Frame, problem: string): StepResult {
  return { frame, problem, added: null };
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
    added: expr.alias,
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
  return { frame: reordered(frame, order), problem: null, added: null };
}

function filter(frame: Frame, step: FilterStep): StepResult {
  const column = findColumn(frame, step.column);
  if (column === null) return failed(frame, `There is no column "${step.column}".`);
  const kept: number[] = [];
  for (const [row, cell] of column.cells.entries()) {
    // A comparison with a null is null in Polars, and a filter drops it.
    if (cell === null || step.value === null) continue;
    if ((cell === step.value) === (step.comparison === '==')) kept.push(row);
  }
  return { frame: reordered(frame, kept), problem: null, added: null };
}

function runStep(frame: Frame, step: Step): StepResult {
  switch (step.verb) {
    case 'with_columns':
      return withColumns(frame, step);
    case 'sort':
      return sort(frame, step);
    case 'filter':
      return filter(frame, step);
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
      return { id, verb, expr: { column: first, methods: [], alias: 'new_column' } };
    case 'sort':
      return { id, verb, by: [first], descending: false };
    case 'filter':
      return {
        id,
        verb,
        column: first,
        comparison: '==',
        value: distinctValues(frame, first)[0] ?? null,
      };
    default:
      return assertNever(verb);
  }
}

/** Returns `value` as Python would write it. */
export function literalText(value: Cell): string {
  if (value === null) return 'None';
  return typeof value === 'number' ? String(value) : quoted(value);
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
      return `.filter(pl.col(${quoted(step.column)}) ${step.comparison} ${literalText(step.value)})`;
    default:
      return assertNever(step);
  }
}
