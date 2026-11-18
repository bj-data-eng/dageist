import { describe, expect, it } from 'vitest';

import type { ExpressionDtypesView, RunView, SegmentView, StepReportView } from './contract';
import { DecodeError } from './decode';
import { decodeRun } from './decode-run';
import incompleteFixture from '../fixtures/step-report-incomplete.json';
import refusedFixture from '../fixtures/step-report-refused.json';
import reportFixture from '../fixtures/step-report.json';
import unreadableFixture from '../fixtures/step-report-unreadable.json';
import variantsFixture from '../fixtures/step-report-variants.json';
import { replaced, withField, without } from '../test-support/tamper';

// The fixtures are what the host's `run_document` command answers for the claim-timing steps,
// for a document with a step Polars refuses, and for text that is not a document, written from
// Rust by its snapshot test. They are read as `unknown`, as the host's answer is, and the
// decoder is what says what they are.
const report: unknown = reportFixture;
const refused: unknown = refusedFixture;
const unreadable: unknown = unreadableFixture;
const variants: unknown = variantsFixture;

/** The steps of a run that was read. */
function stepsOf(run: RunView): readonly StepReportView[] {
  if (run.status !== 'run') throw new Error('the document was not read');
  return run.steps;
}

function stepNamed(run: RunView, id: string): StepReportView {
  const found = stepsOf(run).find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found;
}

/** The dtypes of the expression of `step` that is the argument at `path`. */
function expressionAt(step: StepReportView, path: readonly SegmentView[]): ExpressionDtypesView {
  const found = step.expressions.find(
    (expression) => JSON.stringify(expression.place.path) === JSON.stringify(path),
  );
  if (found === undefined) throw new Error(`no expression at ${JSON.stringify(path)}`);
  return found;
}

/** The expression a step computes into its first new column: the value of the first pair. */
const FIRST_VALUE: readonly SegmentView[] = [
  { kind: 'parameter', name: 'named_exprs' },
  { kind: 'value', index: 0 },
];

function refusal(value: unknown): DecodeError {
  try {
    decodeRun(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

describe('the report of running the claim-timing steps', () => {
  const run = decodeRun(report);

  it('has a step for each of the twelve, valid, and only the pivot collects', () => {
    expect(stepsOf(run).map((step) => step.id)).toEqual([
      'sort',
      'step_index',
      'resolved_at',
      'received_at',
      'resolved_day',
      'days_open',
      'a_days',
      'join',
      'sla_status',
      'flag',
      'holiday_count',
      'pivot',
    ]);
    for (const step of stepsOf(run)) {
      expect(step.outcome.status, step.id).toBe('valid');
      expect(step.lazy, step.id).toBe(step.id !== 'pivot');
    }
  });

  it('gives the columns each step leaves, by the engine short dtype', () => {
    expect(run.status === 'run' && run.source.map((column) => column.name)).toEqual([
      'dcn',
      'timestamp',
      'step_to',
      'owner',
      'row_days',
    ]);
    const pivot = stepNamed(run, 'pivot').outcome;
    expect(pivot).toEqual({
      status: 'valid',
      columns: [
        { name: 'dcn', dtype: 'String' },
        { name: 'Intake', dtype: 'Int64' },
        { name: 'Review', dtype: 'Int64' },
        { name: 'Pending', dtype: 'Int64' },
        { name: 'Complete', dtype: 'Int64' },
      ],
      changes: [
        { name: 'Intake', change: 'added' },
        { name: 'Review', change: 'added' },
        { name: 'Pending', change: 'added' },
        { name: 'Complete', change: 'added' },
      ],
    });
    const added = stepNamed(run, 'received_at').outcome;
    expect(added.status === 'valid' && added.columns.slice(5)).toEqual([
      { name: 'step_index', dtype: 'UInt32' },
      { name: 'resolved_at', dtype: 'Datetime(us)' },
      { name: 'received_at', dtype: 'Datetime(us)' },
    ]);
  });

  it('counts the rows each step kept of those it received, and writes the first values it added', () => {
    expect(stepNamed(run, 'sort').count).toEqual({
      kind: 'counted',
      received: 7,
      kept: 7,
      first: [],
    });
    expect(stepNamed(run, 'resolved_day').count).toEqual({
      kind: 'counted',
      received: 7,
      kept: 7,
      first: [
        {
          name: 'resolved_day',
          values: { shown: [null, '2026-03-02', '2026-03-03', '2026-03-05'], more: 3 },
        },
      ],
    });
    // Two claims are what the seven rows make of the pivot.
    const pivot = stepNamed(run, 'pivot').count;
    expect(pivot.kind === 'counted' && [pivot.received, pivot.kept]).toEqual([7, 2]);
  });

  it('gives the dtype after each link of an expression and where the expression is', () => {
    // `days_open` takes the difference of two timestamps, a duration, and its whole days.
    expect(expressionAt(stepNamed(run, 'days_open'), FIRST_VALUE)).toEqual({
      place: {
        stepIndex: 5,
        stepId: 'days_open',
        path: [
          { kind: 'parameter', name: 'named_exprs' },
          { kind: 'value', index: 0 },
        ],
      },
      start: { kind: 'dtype', dtype: 'Datetime(us)' },
      links: [
        { kind: 'dtype', dtype: 'Duration(us)' },
        { kind: 'dtype', dtype: 'Int64' },
      ],
    });
  });

  it('says a condition has no dtype until it has its then, and element has none outside a pivot', () => {
    expect(expressionAt(stepNamed(run, 'a_days'), FIRST_VALUE).start).toEqual({
      kind: 'unfinished',
    });
    const aggregate = expressionAt(stepNamed(run, 'pivot'), [
      { kind: 'parameter', name: 'aggregate_function' },
    ]);
    expect(aggregate.start).toEqual({ kind: 'noContext' });
    expect(aggregate.links).toEqual([{ kind: 'noContext' }]);
  });

  it('gives what the tables say of each call of an expression', () => {
    const walked = stepNamed(run, 'resolved_day').walk.expressions[0];
    expect(walked?.context).toEqual({ outer: 'select', nested: null });
    expect(walked?.calls[1]?.follows).toEqual({
      kind: 'value',
      dtype: 'Datetime(us)',
      shape: 'many',
    });
    expect(walked?.calls[1]?.verdict).toEqual({
      kind: 'allowed',
      result: 'Date',
      shape: 'many',
      ending: { kind: 'allowed' },
    });
    expect(walked?.ending).toEqual({ kind: 'allowed' });
    expect(stepNamed(run, 'resolved_day').walk.refusal).toBeNull();
  });

  it('asks the tables about every step of the sample, so none says it could not be asked', () => {
    // A number typed without a dtype, as `shift(1)` and `otherwise 0` hold, is asked as the dtype
    // Polars gives it, and a connected input as a literal of its dtype.
    for (const step of run.status === 'run' ? run.steps : []) {
      expect(step.walk.notKnown, step.id).toBeNull();
    }
  });

  it('asks the tables about a chain that starts from a connected list, as a literal of its dtype', () => {
    // `holidays` is a list of dates: many values of a date, and its length is one count.
    const walked = stepNamed(run, 'holiday_count').walk.expressions[0];
    expect(stepNamed(run, 'holiday_count').walk.notKnown).toBeNull();
    expect(walked?.calls[0]).toEqual({
      follows: { kind: 'start' },
      verdict: { kind: 'allowed', result: 'Date', shape: 'many', ending: { kind: 'allowed' } },
    });
    expect(walked?.calls[1]?.follows).toEqual({ kind: 'value', dtype: 'Date', shape: 'many' });
    expect(walked?.calls[1]?.verdict).toEqual({
      kind: 'allowed',
      result: 'UInt32',
      shape: 'one',
      ending: { kind: 'allowed' },
    });
    expect(walked?.places).toEqual([
      {
        context: { outer: 'select', nested: null },
        after: { kind: 'value', dtype: 'Date', shape: 'many' },
      },
      {
        context: { outer: 'select', nested: null },
        after: { kind: 'value', dtype: 'UInt32', shape: 'one' },
      },
    ]);
  });
});

describe('the Python source and the summary of each step', () => {
  const run = decodeRun(report);

  it('gives each step the call it stands for and what it works on, as the engine wrote them', () => {
    expect(stepNamed(run, 'sort')).toMatchObject({
      source: '.sort("dcn", "timestamp")',
      summary: 'by dcn, timestamp',
    });
    expect(stepNamed(run, 'step_index')).toMatchObject({
      source: '.with_columns(pl.col("dcn").cum_count().over(pl.col("dcn")).alias("step_index"))',
      summary: 'step_index',
    });
    expect(stepNamed(run, 'join')).toMatchObject({
      source:
        '.join(step_sla, how="left", left_on="step_to", right_on="step", maintain_order="left")',
      summary: 'step_sla on step_to',
    });
    expect(stepNamed(run, 'pivot')).toMatchObject({ summary: 'on step_to' });
  });

  it('gives the module the document is written as, line by line, each step line naming its step', () => {
    if (run.status !== 'run') throw new Error('the document was not read');
    expect(run).toMatchObject({
      python: expect.arrayContaining([
        { text: 'from datetime import date', step: null },
        { text: '    claims: pl.LazyFrame,', step: null },
        { text: '        .sort("dcn", "timestamp")', step: 'sort' },
      ]) as unknown,
    });
  });

  it('refuses a step that lacks its source or has one that is not text', () => {
    expect(() => decodeRun(without(report, ['steps', 0], 'source'))).toThrow(DecodeError);
    expect(() => decodeRun(replaced(report, ['steps', 0, 'summary'], 3))).toThrow(DecodeError);
  });
});

describe('the report of a document with a step Polars refuses', () => {
  const run = decodeRun(refused);

  it('places the refusal at the link, with Polars own reason', () => {
    const outcome = stepNamed(run, 'total').outcome;
    if (outcome.status !== 'refused') throw new Error('the step was not refused');
    expect(outcome.place).toEqual({
      stepIndex: 1,
      stepId: 'total',
      path: [
        { kind: 'parameter', name: 'named_exprs' },
        { kind: 'value', index: 0 },
        { kind: 'link', index: 0 },
      ],
    });
    expect(outcome.cause.kind).toBe('polars');
    expect(outcome.cause.kind === 'polars' && outcome.cause.reason).toContain(
      '`sum` operation not supported for dtype `str`',
    );
  });

  it('does not run the step after it, and names the one that blocked it', () => {
    expect(stepNamed(run, 'after').outcome).toEqual({ status: 'notRun', blockedBy: 'total' });
    expect(stepNamed(run, 'sort').outcome.status).toBe('valid');
  });

  it('gives the refusal the tables found and who made it', () => {
    const walk = stepNamed(run, 'total').walk;
    expect(walk.refusal).toEqual({
      place: {
        stepIndex: 1,
        stepId: 'total',
        path: [
          { kind: 'parameter', name: 'named_exprs' },
          { kind: 'value', index: 0 },
          { kind: 'link', index: 0 },
        ],
      },
      refusal: {
        source: { by: 'polars', stage: 'schema' },
        reason: '`sum` operation not supported for dtype `str`',
      },
      atEnd: false,
    });
    // The expression is walked to the call that is refused and no further, so it has no end.
    expect(walk.expressions[0]?.ending).toBeNull();
  });
});

describe('the report of a document with a step that holds something still to be picked', () => {
  const run = decodeRun(incompleteFixture);
  const steps = run.status === 'run' ? run.steps : [];

  it('says the step is incomplete at the start of the expression that is a hole', () => {
    expect(steps.map((step) => step.outcome.status)).toEqual(['valid', 'incomplete', 'notRun']);
    expect(steps[1]?.outcome).toEqual({
      status: 'incomplete',
      place: {
        stepIndex: 1,
        stepId: 'later',
        path: [
          { kind: 'parameter', name: 'named_exprs' },
          { kind: 'value', index: 0 },
          { kind: 'link', index: 0 },
          { kind: 'parameter', name: 'n' },
          { kind: 'start' },
        ],
      },
    });
  });

  it('says the next step was not run, and because of which', () => {
    expect(steps[2]?.outcome).toEqual({ status: 'notRun', blockedBy: 'later' });
  });

  it('walks the incomplete step up to the hole: the shift stands after a timestamp', () => {
    const value = steps[1]?.walk.expressions.find(
      (expression) => expression.place.path.length === 2,
    );
    expect(value?.places[0]).toEqual({
      context: { outer: 'select', nested: null },
      after: { kind: 'value', dtype: 'Datetime(us)', shape: 'many' },
    });
  });
});

describe('the report of text that is not a document', () => {
  it('has the place and the reason, and no steps', () => {
    expect(decodeRun(unreadable)).toEqual({
      status: 'unreadable',
      problem: {
        kind: 'refused',
        place: { stepIndex: null, stepId: null, path: [] },
        reason: 'version 2 is not one this engine reads; it reads version 1',
      },
    });
  });

  it('reads a syntax problem with its line and column', () => {
    const syntax = {
      status: 'unreadable',
      problem: { kind: 'syntax', line: 2, column: 10, reason: 'the text ends too soon' },
    };
    expect(decodeRun(syntax)).toEqual(syntax);
  });
});

describe('the report built by hand for the variants no document gives together', () => {
  // The host writes it from views it builds itself, so each variant is read in the shape the
  // host really serializes it, and each expectation below is written out by hand.
  const run = decodeRun(variants);
  const refusedCause = (id: string) => {
    const outcome = stepNamed(run, id).outcome;
    if (outcome.status !== 'refused') throw new Error(`${id} was not refused`);
    return outcome.cause;
  };

  it('reads a start that has several columns', () => {
    expect(stepNamed(run, 'variants').expressions[0]).toEqual({
      place: {
        stepIndex: 0,
        stepId: 'variants',
        path: [
          { kind: 'parameter', name: 'named_exprs' },
          { kind: 'key', index: 1 },
          { kind: 'start' },
        ],
      },
      start: { kind: 'several', dtypes: ['Int64', 'String'] },
      links: [{ kind: 'refused', reason: 'no' }],
    });
  });

  it('reads a condition that has been chained, and a context two positions deep', () => {
    const walked = stepNamed(run, 'variants').walk.expressions[0];
    expect(walked?.context).toEqual({ outer: 'filter', nested: 'over.order_by' });
    expect(walked?.calls).toEqual([
      {
        follows: { kind: 'condition', state: 'chainedWhen' },
        verdict: {
          kind: 'refused',
          refusal: { source: { by: 'structure' }, reason: 'the reason' },
        },
      },
      { follows: { kind: 'condition', state: 'chainedThen' }, verdict: null },
    ]);
    expect(walked?.places).toEqual([
      {
        context: { outer: 'sort', nested: null },
        after: { kind: 'condition', state: 'chainedThen' },
      },
      null,
    ]);
  });

  it('reads an ending the tables do not know and a refusal at the end', () => {
    const walk = stepNamed(run, 'variants').walk;
    expect(walk.expressions[0]?.ending).toEqual({
      kind: 'notKnown',
      because: 'the dtype is not known',
    });
    expect(walk.refusal?.refusal.source).toEqual({ by: 'polars', stage: 'empty' });
    expect(walk.refusal?.atEnd).toBe(true);
    expect(walk.notKnown).toBe('why');
    expect(stepNamed(run, 'variants').lazy).toBe(false);
  });

  it('reads each cause a step can be refused with', () => {
    expect(refusedCause('unknown_table')).toEqual({ kind: 'unknownTable', name: 'nope' });
    expect(refusedCause('unknown_input')).toEqual({ kind: 'unknownInput', name: 'none' });
    expect(refusedCause('build')).toEqual({ kind: 'build', reason: 'a parameter is missing' });
    expect(refusedCause('rule')).toEqual({
      kind: 'notAllowed',
      refusal: {
        source: { by: 'rule', rule: 'element-only-in-pivot-aggregate' },
        reason: 'the reason',
      },
    });
    expect(refusedCause('rows_stage')).toEqual({
      kind: 'notAllowed',
      refusal: { source: { by: 'polars', stage: 'rows' }, reason: 'the reason' },
    });
  });

  it('places a refusal at the start of an expression', () => {
    const outcome = stepNamed(run, 'build').outcome;
    expect(outcome.status === 'refused' && outcome.place).toEqual({
      stepIndex: 1,
      stepId: 'build',
      path: [{ kind: 'start' }],
    });
  });
});

describe('a report that is not what the host sends', () => {
  it('is refused where the status is not one the host has', () => {
    expect(refusal(replaced(report, ['status'], 'halted')).message).toBe(
      '$.status: must be one of "run", "unreadable", not "halted"',
    );
    expect(refusal(replaced(report, ['steps', 3, 'outcome', 'status'], 'skipped')).message).toBe(
      '$.steps[3].outcome.status: must be one of "valid", "refused", "incomplete", "notRun", not "skipped"',
    );
  });

  it('is refused where a field has another kind', () => {
    expect(refusal(replaced(report, ['steps', 0, 'lazy'], 'yes')).message).toBe(
      '$.steps[0].lazy: must be a boolean, not a string',
    );
    expect(
      refusal(replaced(report, ['steps', 0, 'outcome', 'columns', 0, 'dtype'], null)).message,
    ).toBe('$.steps[0].outcome.columns[0].dtype: must be a string, not null');
    expect(refusal([]).message).toBe('$: must be an object, not a list');
  });

  it('is refused where a position is not a whole number from zero', () => {
    expect(
      refusal(replaced(report, ['steps', 4, 'expressions', 0, 'place', 'path', 1, 'index'], -1))
        .message,
    ).toBe('$.steps[4].expressions[0].place.path[1].index: must be a whole number from 0, not -1');
    expect(
      refusal(replaced(report, ['steps', 4, 'expressions', 0, 'place', 'stepIndex'], 1.5)).problem,
    ).toBe('must be a whole number from 0, not 1.5');
  });

  it('is refused where a field is missing or one the contract lacks is there', () => {
    expect(refusal(without(report, ['steps', 2, 'walk'], 'notKnown')).message).toBe(
      '$.steps[2].walk: is missing the field "notKnown"',
    );
    expect(refusal(withField(report, ['steps', 2, 'walk'], 'cost', 1)).message).toBe(
      '$.steps[2].walk: has the field "cost", which the contract lacks',
    );
    expect(refusal(without(report, [], 'source')).message).toBe('$: is missing the field "source"');
  });

  it('is refused where a rule, a context or a dtype resolution is not one the engine has', () => {
    const source = ['steps', 1, 'walk', 'refusal', 'refusal', 'source'];
    expect(refusal(replaced(refused, source, { by: 'rule', rule: 'no-such-rule' })).problem).toBe(
      'must be one of "pivot-aggregate-gives-one-value", "condition-predicate-is-boolean", "element-only-in-pivot-aggregate", "filter-predicate-is-boolean", "join-key-names-are-not-repeated", "key-is-elementwise", not "no-such-rule"',
    );
    expect(
      refusal(
        replaced(report, ['steps', 4, 'walk', 'expressions', 0, 'context', 'outer'], 'having'),
      ).path,
    ).toBe('$.steps[4].walk.expressions[0].context.outer');
    expect(
      refusal(replaced(report, ['steps', 4, 'expressions', 0, 'start'], { kind: 'maybe' })).path,
    ).toBe('$.steps[4].expressions[0].start.kind');
  });

  it('is refused where the fields of one variant are those of another', () => {
    expect(refusal(withField(unreadable, ['problem'], 'line', 1)).message).toBe(
      '$.problem: has the field "line", which the contract lacks',
    );
    expect(
      refusal(replaced(unreadable, ['problem'], { kind: 'syntax', place: null, reason: 'x' }))
        .message,
    ).toBe('$.problem: is missing the field "line"');
  });
});

// A report written by hand for what no one document gives together: each way a step's rows were
// counted, and a step that replaced a column.
function handWrittenStep(count: unknown): unknown {
  return {
    id: 'add',
    lazy: true,
    outcome: {
      status: 'valid',
      columns: [
        { name: 'owner', dtype: 'String' },
        { name: 'slow', dtype: 'Boolean' },
      ],
      changes: [
        { name: 'owner', change: 'replaced' },
        { name: 'slow', change: 'added' },
      ],
    },
    count,
    expressions: [],
    walk: { expressions: [], refusal: null, notKnown: null },
  };
}

const COUNTED = {
  kind: 'counted',
  received: 7,
  kept: 4,
  first: [{ name: 'slow', values: { shown: ['true', null], more: 2 } }],
};

function runOf(count: unknown): unknown {
  return { status: 'run', source: [], steps: [handWrittenStep(count)] };
}

describe('the columns a step changed and how its rows were counted', () => {
  it('are read as the host sends them, each way a count can be', () => {
    for (const count of [COUNTED, { kind: 'notCounted' }, { kind: 'failed', reason: 'no' }]) {
      expect(stepsOf(decodeRun(runOf(count)))[0]).toEqual(handWrittenStep(count));
    }
  });

  it('are refused for a change, a count or a value that the contract does not name', () => {
    const changed = replaced(
      runOf(COUNTED),
      ['steps', 0, 'outcome', 'changes', 0, 'change'],
      'removed',
    );
    expect(refusal(changed).message).toBe(
      '$.steps[0].outcome.changes[0].change: must be one of "added", "replaced", not "removed"',
    );
    expect(
      refusal(replaced(runOf(COUNTED), ['steps', 0, 'count', 'kind'], 'partial')).message,
    ).toBe(
      '$.steps[0].count.kind: must be one of "notCounted", "counted", "failed", not "partial"',
    );
    expect(refusal(replaced(runOf(COUNTED), ['steps', 0, 'count', 'kept'], 'four')).message).toBe(
      '$.steps[0].count.kept: must be a whole number, not a string',
    );
    expect(refusal(replaced(runOf(COUNTED), ['steps', 0, 'count', 'received'], -1)).message).toBe(
      '$.steps[0].count.received: must be a whole number from 0, not -1',
    );
    expect(
      refusal(without(runOf(COUNTED), ['steps', 0, 'count', 'first', 0, 'values'], 'more')).message,
    ).toBe('$.steps[0].count.first[0].values: is missing the field "more"');
    expect(
      refusal(withField(runOf({ kind: 'notCounted' }), ['steps', 0, 'count'], 'kept', 1)).message,
    ).toBe('$.steps[0].count: has the field "kept", which the contract lacks');
    expect(refusal(without(runOf(COUNTED), ['steps', 0], 'count')).message).toBe(
      '$.steps[0]: is missing the field "count"',
    );
    expect(refusal(without(runOf(COUNTED), ['steps', 0, 'outcome'], 'changes')).message).toBe(
      '$.steps[0].outcome: is missing the field "changes"',
    );
  });
});
