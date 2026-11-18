import { describe, expect, it } from 'vitest';

import { decodeCatalogue, decodeNode, decodeRun } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import nodeFixture from '../../../host/fixtures/node-inputs.json';
import refusedFixture from '../../../host/fixtures/step-report-refused.json';
import reportFixture from '../../../host/fixtures/step-report.json';
import incompleteFixture from '../../../host/fixtures/step-report-incomplete.json';
import unreadableFixture from '../../../host/fixtures/step-report-unreadable.json';
import { START } from '../start';
import type { Catalogue } from './catalogue';
import { indexCatalogue } from './catalogue';
import { translate, wrongId } from './document';
import { NO_OPTIONS } from './settings';
import { fieldKeys } from './steps';
import { HostError } from '@dageist/host';
import type { Answered } from './report';
import { prepareRun, rowsFailure, rowsToShow } from './report';
import type { Step } from './steps';

// The report the host writes of the claim-timing steps, and what the editor shows of it.
const catalogue: Catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const source = decodeNode(nodeFixture).source.columns;
const run = decodeRun(reportFixture);

/** The engine's report of the document `translation` is, as the editor gets it. */
function answeredFor(
  translation: ReturnType<typeof translate>,
  report: ReturnType<typeof decodeRun>,
): Answered {
  return { document: translation.text, run: report };
}

/** The editor's starting steps, prepared against the engine's report of them. */
function started(): ReturnType<typeof prepareRun> {
  return prepareRun(
    START,
    translate(catalogue, START),
    answeredFor(translate(catalogue, START), run),
    source,
  );
}

/** The step of the editor called `id`, prepared. */
function stepOf(
  prepared: ReturnType<typeof prepareRun>,
  id: string,
): ReturnType<typeof prepareRun>['steps'][number] {
  const found = prepared.steps.find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found;
}

/** The identifier of the chain a column added by step `id` holds. */
function chainIdOf(id: string): string {
  const step = START.find((candidate) => candidate.id === id);
  if (step?.verb !== 'with_columns' || step.value.kind !== 'chain') {
    throw new Error(`step ${id} holds no chain`);
  }
  return step.value.id;
}

/** A report of one step that names `timestamp` and the engine says changed `changes`. */
function timestampRun(changes: ReadonlyArray<{ name: string; change: string }>): {
  readonly steps: Step[];
  readonly run: ReturnType<typeof decodeRun>;
} {
  const steps: Step[] = [
    {
      id: 'timestamp',
      verb: 'with_columns',
      alias: 'timestamp',
      value: { kind: 'column', column: 'timestamp' },
      options: NO_OPTIONS,
    },
  ];
  return {
    steps,
    run: decodeRun({
      status: 'run',
      source,
      steps: [
        {
          id: 'timestamp',
          lazy: true,
          outcome: { status: 'valid', columns: source, changes },
          count: { kind: 'notCounted' },
          expressions: [],
          walk: { expressions: [], refusal: null, notKnown: null },
        },
      ],
    }),
  };
}

describe('the columns the editor shows after each step', () => {
  it('are the engine’s columns of the step, and the columns the step is given are the step before’s', () => {
    const prepared = started();

    expect(prepared.source).toEqual(source);
    expect(stepOf(prepared, 'sort').before).toEqual(source);
    expect(stepOf(prepared, 'step_index').before).toEqual(stepOf(prepared, 'sort').after);
    expect(
      stepOf(prepared, 'step_index').after.map((column) => `${column.name}:${column.dtype}`),
    ).toEqual([
      'dcn:String',
      'timestamp:Datetime(us)',
      'step_to:String',
      'owner:String',
      'row_days:Int64',
      'step_index:UInt32',
    ]);
  });

  it('say which columns a step added, as the engine reports them', () => {
    const prepared = started();

    expect(stepOf(prepared, 'sort')).toMatchObject({ changes: [] });
    expect(stepOf(prepared, 'step_index')).toMatchObject({
      changes: [{ name: 'step_index', change: 'added' }],
    });
    expect(stepOf(prepared, 'join')).toMatchObject({
      changes: [{ name: 'sla_days', change: 'added' }],
    });
    expect(stepOf(prepared, 'pivot')).toMatchObject({
      changes: ['Intake', 'Review', 'Pending', 'Complete'].map((name) => ({
        name,
        change: 'added',
      })),
    });
  });

  it('say a column is replaced where the engine says so, with the dtype it had', () => {
    const { steps, run: replaced } = timestampRun([{ name: 'timestamp', change: 'replaced' }]);
    const translation = translate(catalogue, steps);

    const prepared = prepareRun(steps, translation, answeredFor(translation, replaced), source);

    expect(prepared.steps[0]).toMatchObject({
      changes: [{ name: 'timestamp', change: 'replaced' }],
    });
  });

  it('do not add a column the step names to the changes the engine reports', () => {
    const { steps, run: unchanged } = timestampRun([]);
    const translation = translate(catalogue, steps);

    const prepared = prepareRun(steps, translation, answeredFor(translation, unchanged), source);

    expect(prepared.steps[0]).toMatchObject({ changes: [] });
  });

  it('say which steps stay lazy, as the engine reports it', () => {
    const prepared = started();

    expect(stepOf(prepared, 'join').lazy).toBe(true);
    expect(stepOf(prepared, 'pivot').lazy).toBe(false);
  });
});

/** A report of two valid steps, the second of which the engine counted as `count`. */
function counted(count: unknown): ReturnType<typeof decodeRun> {
  return decodeRun({
    status: 'run',
    source,
    steps: ['sort', 'step_index'].map((id) => ({
      id,
      lazy: true,
      outcome: { status: 'valid', columns: source, changes: [] },
      count: id === 'sort' ? { kind: 'notCounted' } : count,
      expressions: [],
      walk: { expressions: [], refusal: null, notKnown: null },
    })),
  });
}

describe('a step the engine could not run on the rows', () => {
  it('says what Polars said, as far as the first paragraph, which is before its plan', () => {
    const steps = START.slice(0, 2);
    const translation = translate(catalogue, steps);
    const reason = 'the date of text is not a date\n\nResolved plan until failure: …';

    const prepared = prepareRun(
      steps,
      translation,
      answeredFor(translation, counted({ kind: 'failed', reason })),
      source,
    );

    expect(prepared.steps[1]).toMatchObject({
      status: 'valid',
      rowsProblem: 'Could not be run on the sample rows: the date of text is not a date',
    });
    // The step before it was not counted, which is not a failure.
    expect(prepared.steps[0]).toMatchObject({ rowsProblem: null });
  });

  it('is nothing for a step that was counted or not counted, or not reported on', () => {
    const prepared = started();

    for (const step of prepared.steps) {
      expect(step, step.id).toMatchObject({ rowsProblem: null });
    }
  });
});

describe('the rows each step kept and the first values it added', () => {
  it('are how many rows the step kept of those it received, where the engine counted them', () => {
    const prepared = started();

    expect(stepOf(prepared, 'sort')).toMatchObject({ rows: { received: 7, kept: 7 } });
    // Seven claim rows make two claims.
    expect(stepOf(prepared, 'pivot')).toMatchObject({ rows: { received: 7, kept: 2 } });
  });

  it('are the first values of each column a step changed, as one line of the text the engine wrote', () => {
    const prepared = started();

    // A null is a dash, and a column with more values than are shown ends in an ellipsis.
    expect(stepOf(prepared, 'step_index')).toMatchObject({
      first: new Map([['step_index', '1, 2, 3, 4, …']]),
    });
    expect(stepOf(prepared, 'resolved_day')).toMatchObject({
      first: new Map([['resolved_day', '—, 2026-03-02, 2026-03-03, 2026-03-05, …']]),
    });
    expect(stepOf(prepared, 'join')).toMatchObject({
      first: new Map([['sla_days', '1, 1, 2, 3, …']]),
    });
    // The two rows of the pivot are all there are: no ellipsis.
    expect(stepOf(prepared, 'pivot')).toMatchObject({
      first: new Map([
        ['Intake', '0, 0'],
        ['Review', '1, 2'],
        ['Pending', '2, —'],
        ['Complete', '2, 2'],
      ]),
    });
  });

  it('are nothing for a step the engine did not count, or could not run on the rows', () => {
    const steps = START.slice(0, 2);
    const translation = translate(catalogue, steps);
    for (const count of [{ kind: 'notCounted' }, { kind: 'failed', reason: 'no rows' }]) {
      const prepared = prepareRun(
        steps,
        translation,
        answeredFor(translation, counted(count)),
        source,
      );
      expect(prepared.steps[0]).toMatchObject({ rows: null, first: new Map() });
      expect(prepared.steps[1]).toMatchObject({ rows: null, first: new Map() });
    }
  });
});

/** The place a link stands in, in a select, after a value of `dtype`. */
function selectAfter(dtype: string, shape: 'one' | 'many') {
  return {
    context: { outer: 'select', nested: null },
    after: { kind: 'value', dtype, shape },
  } as const;
}

describe('the dtype the editor shows after each link of a chain', () => {
  it('is the engine’s dtype after the start and after each method', () => {
    const prepared = started();

    expect(stepOf(prepared, 'step_index').chains.get(chainIdOf('step_index'))).toEqual({
      source: 'String',
      methods: ['UInt32', 'UInt32'],
      // Where each method stands, and where one added at the end would: after a text, then after
      // a count, for each row, twice.
      places: [
        selectAfter('String', 'many'),
        selectAfter('UInt32', 'many'),
        selectAfter('UInt32', 'many'),
      ],
    });
    expect(stepOf(prepared, 'resolved_day').chains.get(chainIdOf('resolved_day'))).toEqual({
      source: 'Datetime(us)',
      methods: ['Date'],
      places: [selectAfter('Datetime(us)', 'many'), selectAfter('Date', 'many')],
    });
  });

  it('is the dtype once the otherwise is there, for a chain that starts from a when', () => {
    const prepared = started();

    expect(stepOf(prepared, 'a_days').chains.get(chainIdOf('a_days'))).toEqual({
      source: 'Int64',
      methods: ['Int64', 'Int64'],
      // The values of the condition are whole numbers and a number typed without a dtype, which
      // takes theirs, so the engine knows what follows `otherwise` and says it gives one for each
      // row, and that it still does after the two methods that follow.
      places: [
        selectAfter('Int64', 'many'),
        selectAfter('Int64', 'many'),
        selectAfter('Int64', 'many'),
      ],
    });
  });

  it('is the dtype of the input a chain starts from, with the methods that follow it', () => {
    const prepared = started();

    expect(stepOf(prepared, 'holiday_count').chains.get(chainIdOf('holiday_count'))).toEqual({
      source: 'Date',
      methods: ['UInt32'],
      // A list of dates gives many; its length is one count.
      places: [selectAfter('Date', 'many'), selectAfter('UInt32', 'one')],
    });
  });

  it('is nothing where the engine gives no dtype', () => {
    const prepared = started();
    const pivot = START.find((step) => step.id === 'pivot');
    if (pivot?.verb !== 'pivot') throw new Error('no pivot');
    const aggregate = pivot.options.aggregate_function;
    if (aggregate?.kind !== 'expression' || aggregate.value.kind !== 'chain') {
      throw new Error('no aggregate');
    }

    // `element` has no dtype outside the cell of a pivot.
    expect(stepOf(prepared, 'pivot').chains.get(aggregate.value.id)).toEqual({
      source: null,
      methods: [null],
      // The tables know what a group holds: the field the pivot takes its values from.
      places: [
        {
          context: { outer: 'pivot_aggregate', nested: null },
          after: { kind: 'value', dtype: 'Int64', shape: 'many' },
        },
        {
          context: { outer: 'pivot_aggregate', nested: null },
          after: { kind: 'value', dtype: 'Int64', shape: 'one' },
        },
      ],
    });
  });
});

describe('a refusal', () => {
  // The claims sorted, then a sum of a text column, which Polars refuses, then a step never run.
  const refusedSteps: Step[] = [
    {
      id: 'sort',
      verb: 'sort',
      options: START[0]?.verb === 'sort' ? START[0].options : NO_OPTIONS,
    },
    {
      id: 'total',
      verb: 'with_columns',
      alias: 'total',
      value: {
        kind: 'chain',
        id: 'chain-total',
        chain: {
          source: { kind: 'column', column: 'dcn' },
          methods: [{ id: 'method-sum', name: 'sum', argument: null, options: NO_OPTIONS }],
        },
      },
      options: NO_OPTIONS,
    },
    {
      id: 'after',
      verb: 'with_columns',
      alias: 'days',
      value: { kind: 'column', column: 'row_days' },
      options: NO_OPTIONS,
    },
  ];
  const refusedTranslation = translate(catalogue, refusedSteps);
  const refused = prepareRun(
    refusedSteps,
    refusedTranslation,
    answeredFor(refusedTranslation, decodeRun(refusedFixture)),
    source,
  );

  it('is shown on the word it marks: here, the method the engine refused', () => {
    const total = stepOf(refused, 'total');

    expect(total.status).toBe('refused');
    expect(total.wrong).toEqual(new Set([wrongId({ kind: 'method', methodId: 'method-sum' })]));
  });

  it('says what Polars said of it, as far as the first paragraph, which is before its plan', () => {
    expect(stepOf(refused, 'total').problem).toBe('`sum` operation not supported for dtype `str`');
  });

  it('gives the step the columns it was given, and the next step is not run', () => {
    const total = stepOf(refused, 'total');

    expect(total.after).toEqual(total.before);
    expect(stepOf(refused, 'after')).toMatchObject({
      status: 'notRun',
      problem: 'Not run: a step before it was refused.',
    });
  });

  it('leaves the steps before it as they were', () => {
    expect(stepOf(refused, 'sort').status).toBe('valid');
    expect(stepOf(refused, 'sort').problem).toBeNull();
  });
});

describe('a step the document cannot hold', () => {
  const steps: Step[] = [
    {
      id: 'first',
      verb: 'with_columns',
      alias: 'first',
      value: { kind: 'column', column: 'dcn' },
      options: NO_OPTIONS,
    },
    {
      id: 'second',
      verb: 'with_columns',
      alias: 'second',
      value: { kind: 'literal', value: null },
      options: NO_OPTIONS,
    },
    {
      id: 'third',
      verb: 'with_columns',
      alias: 'third',
      value: { kind: 'column', column: 'dcn' },
      options: NO_OPTIONS,
    },
  ];

  it('is refused with what is the matter, and the steps after it are not run, before the engine is asked', () => {
    const prepared = prepareRun(steps, translate(catalogue, steps), null, source);

    expect(prepared.steps.map((step) => step.status)).toEqual(['unknown', 'refused', 'notRun']);
    expect(prepared.steps[1]?.problem).toBe('The engine does not take None as a value yet.');
    expect(prepared.steps[2]?.problem).toBe('Not run: step 2 cannot be run yet.');
  });

  it('gives every step the columns of the table the steps start from until the engine has reported', () => {
    const prepared = prepareRun(steps, translate(catalogue, steps), null, source);

    expect(prepared.steps.map((step) => step.before)).toEqual([source, source, source]);
  });
});

describe('a document the engine cannot read', () => {
  it('refuses the first step with the engine’s reason, and the steps after it are not run', () => {
    const translation = translate(catalogue, START);
    const prepared = prepareRun(
      START,
      translation,
      answeredFor(translation, decodeRun(unreadableFixture)),
      source,
    );

    expect(prepared.steps[0]).toMatchObject({
      status: 'refused',
      problem: 'version 2 is not one this engine reads; it reads version 1',
    });
    expect(prepared.steps[1]?.status).toBe('notRun');
  });
});

describe('the words for a step that has no rows', () => {
  const step = stepOf(started(), 'sort');

  it('follow the kind of the failure, not its text', () => {
    expect(rowsFailure('noSuchStep', 'there is no step "x"', null)).toBe(
      'This step is not in the document yet.',
    );
    expect(rowsFailure('limit', 'a preview holds at most 1000 rows', null)).toBe(
      'There are more rows than a preview holds.',
    );
    expect(rowsFailure('unreadable', 'reading the document: …', null)).toBe(
      'The steps cannot be read yet.',
    );
  });

  it('say why the step was refused or not run when the report has said', () => {
    const refusedStep = { ...step, status: 'refused' as const, problem: 'the reason' };

    expect(rowsFailure('refused', 'previewing the rows: …', refusedStep)).toBe('the reason');
    expect(rowsFailure('notRun', 'previewing the rows: …', refusedStep)).toBe('the reason');
    expect(rowsFailure('refused', 'previewing the rows: …', null)).toBe('This step was refused.');
  });

  it('keep the message of a failure that has no kind of its own', () => {
    expect(rowsFailure('failed', 'could not collect the rows', null)).toBe(
      'could not collect the rows',
    );
    expect(rowsFailure('unknownName', 'the catalogue has no entry Expr.nothing', null)).toBe(
      'the catalogue has no entry Expr.nothing',
    );
  });
});

describe('the rows shown after a step is chosen', () => {
  const rows = { columns: [{ name: 'a', dtype: 'Int64' }], rows: [['1']] };
  const step = stepOf(started(), 'sort');

  it('are those the engine gave for that step', () => {
    expect(rowsToShow({ step: 'sort', outcome: { ok: true, answer: rows } }, 'sort', step)).toEqual(
      {
        kind: 'rows',
        columns: rows.columns,
        rows: rows.rows,
      },
    );
  });

  it('are nothing yet when the answer is for another step, whether it holds rows or a failure', () => {
    const failed = new HostError('rows_after_step', 'refused', 'previewing the rows: …');

    expect(
      rowsToShow({ step: 'sort', outcome: { ok: true, answer: rows } }, 'join', step),
    ).toBeNull();
    expect(
      rowsToShow({ step: 'sort', outcome: { ok: false, error: failed } }, 'join', step),
    ).toBeNull();
    expect(rowsToShow(null, 'join', step)).toBeNull();
  });

  it('say why there are none by the kind of the failure for the step that was chosen', () => {
    const failed = new HostError('rows_after_step', 'refused', 'previewing the rows: …');
    const refusedStep = { ...step, status: 'refused' as const, problem: 'the reason' };

    expect(
      rowsToShow({ step: 'sort', outcome: { ok: false, error: failed } }, 'sort', refusedStep),
    ).toEqual({
      kind: 'message',
      text: 'the reason',
    });
  });

  it('keep the text of a failure that is not the host’s', () => {
    expect(
      rowsToShow(
        { step: 'sort', outcome: { ok: false, error: new Error('no network') } },
        'sort',
        step,
      ),
    ).toEqual({ kind: 'message', text: 'no network' });
  });
});

/** The starting steps with the step called `id` changed by `change`. */
function edited(id: string, change: (step: Step) => Step): readonly Step[] {
  return START.map((step) => (step.id === id ? change(step) : step));
}

describe('a report of steps that have been edited since', () => {
  const answeredStart = answeredFor(translate(catalogue, START), run);

  it('is shown for the steps before the edited one, with their dtypes, and not after', () => {
    const steps = edited('resolved_day', (step) =>
      step.verb === 'with_columns' ? { ...step, alias: 'day' } : step,
    );

    const prepared = prepareRun(steps, translate(catalogue, steps), answeredStart, source);

    expect(prepared.steps.slice(0, 4).map((step) => step.status)).toEqual([
      'valid',
      'valid',
      'valid',
      'valid',
    ]);
    expect(stepOf(prepared, 'step_index').chains.get(chainIdOf('step_index'))).toMatchObject({
      source: 'String',
      methods: ['UInt32', 'UInt32'],
    });
    // From the edited step on the engine has not said, so no dtype is placed against a link.
    expect(prepared.steps.slice(4).map((step) => step.status)).toEqual(Array(8).fill('unknown'));
    expect(prepared.steps.slice(4).every((step) => step.chains.size === 0)).toBe(true);
    expect(stepOf(prepared, 'resolved_day').problem).toBeNull();
    // Nor are the changes, the counts or the first values of steps the engine has not reported on.
    for (const step of prepared.steps.slice(4)) {
      expect(step, step.id).toMatchObject({ changes: [], rows: null, first: new Map() });
    }
    // The steps before it keep theirs.
    expect(stepOf(prepared, 'step_index')).toMatchObject({
      changes: [{ name: 'step_index', change: 'added' }],
      rows: { received: 7, kept: 7 },
    });
  });

  it('gives the steps that wait the columns of the last step that still corresponds', () => {
    const steps = edited('resolved_day', (step) =>
      step.verb === 'with_columns' ? { ...step, alias: 'day' } : step,
    );

    const prepared = prepareRun(steps, translate(catalogue, steps), answeredStart, source);

    expect(stepOf(prepared, 'resolved_day').before).toEqual(stepOf(prepared, 'received_at').after);
    expect(stepOf(prepared, 'days_open').before).toEqual(stepOf(prepared, 'received_at').after);
  });

  it('does not place the dtypes of the old links on a chain that has gained a method', () => {
    const steps = edited('resolved_day', (step) =>
      step.verb === 'with_columns' && step.value.kind === 'chain'
        ? {
            ...step,
            value: {
              ...step.value,
              chain: {
                ...step.value.chain,
                methods: [
                  { id: 'extra', name: 'dt.time', argument: null, options: NO_OPTIONS },
                  ...step.value.chain.methods,
                ],
              },
            },
          }
        : step,
    );

    const prepared = prepareRun(steps, translate(catalogue, steps), answeredStart, source);

    expect(stepOf(prepared, 'resolved_day').status).toBe('unknown');
    expect(stepOf(prepared, 'resolved_day').chains.size).toBe(0);
  });

  it('is not shown, as a refusal, for a document that is not the one now', () => {
    const steps = edited('sort', (step) =>
      step.verb === 'sort' ? { ...step, options: { by: fieldKeys(['dcn']) } } : step,
    );

    const prepared = prepareRun(
      steps,
      translate(catalogue, steps),
      answeredFor(translate(catalogue, START), decodeRun(unreadableFixture)),
      source,
    );

    expect(prepared.steps[0]?.status).toBe('unknown');
  });
});

describe('a step with something still to be picked', () => {
  // A sort, a column that is the timestamp shifted by something still to be picked, and a column
  // after it: the engine says the second is incomplete and the third was not run.
  const steps: Step[] = [
    { id: 'sort', verb: 'sort', options: { by: fieldKeys(['dcn']) } },
    {
      id: 'later',
      verb: 'with_columns',
      alias: 'earlier',
      value: {
        kind: 'chain',
        id: 'chain-later',
        chain: {
          source: { kind: 'column', column: 'timestamp' },
          methods: [
            { id: 'method-shift', name: 'shift', argument: { kind: 'unset' }, options: NO_OPTIONS },
          ],
        },
      },
      options: NO_OPTIONS,
    },
    {
      id: 'after',
      verb: 'with_columns',
      alias: 'days',
      value: { kind: 'column', column: 'row_days' },
      options: NO_OPTIONS,
    },
  ];
  const translation = translate(catalogue, steps);
  const prepared = prepareRun(
    steps,
    translation,
    answeredFor(translation, decodeRun(incompleteFixture)),
    source,
  );

  it('is said to be incomplete, with what is to be picked, on the word that is the matter', () => {
    expect(prepared.steps.map((step) => step.status)).toEqual(['valid', 'refused', 'notRun']);
    const later = stepOf(prepared, 'later');
    expect(later.problem).toBe('Give shift a value or a field (rows).');
    expect([...later.wrong]).toEqual([wrongId({ kind: 'method', methodId: 'method-shift' })]);
  });

  it('keeps the places of the chain before the hole, where the argument would stand', () => {
    const chain = stepOf(prepared, 'later').chains.get('chain-later');
    // The shift stands after the timestamp, for each row.
    expect(chain?.places[0]).toEqual(selectAfter('Datetime(us)', 'many'));
  });

  it('says why the step after it was not run', () => {
    expect(stepOf(prepared, 'after').problem).toBe('Not run: step 2 cannot be run yet.');
  });
});
