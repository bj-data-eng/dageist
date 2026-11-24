import { describe, expect, it } from 'vitest';

import type { DocumentStepView } from '@dageist/host';
import { decodeDocumentState, decodeRun, HostError } from '@dageist/host';

import documentFixture from '../../../host/fixtures/node-document.json';
import incompleteFixture from '../../../host/fixtures/step-report-incomplete.json';
import refusedFixture from '../../../host/fixtures/step-report-refused.json';
import variantsFixture from '../../../host/fixtures/step-report-variants.json';
import type { Path } from './doc';
import { linkSegment, parameterSegment, pathKey, valueSegment } from './doc';
import type { PreparedRun, PreparedStep } from './report';
import { prepareRun, rowsFailure, rowsToShow } from './report';

// What the host reports of the claim-timing steps, and what the editor shows of it. The fixtures
// are written from Rust; the expectations are written by hand from what the steps do.
const held = decodeDocumentState(documentFixture);
const prepared = prepareRun(held.document.steps, held.run);

/** The step called `id`, prepared. */
function stepOf(run: PreparedRun, id: string): PreparedStep {
  const found = run.steps.find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found;
}

/** Steps with these names, which is all a report of them needs of the document. */
function named(...ids: string[]): DocumentStepView[] {
  return ids.map((id) => ({ id, call: { entry: 'DataFrame.select', arguments: [] } }));
}

/** The way to the expression a column is made from, in a `with_columns`. */
const COLUMN: Path = [parameterSegment('named_exprs'), valueSegment(0)];

/** The place a link stands in, in a select, after a value of `dtype`. */
function selectAfter(dtype: string, shape: 'one' | 'many') {
  return {
    context: { outer: 'select', nested: null },
    after: { kind: 'value', dtype, shape },
  } as const;
}

describe('the columns the editor shows after each step', () => {
  it('are the engine’s columns of the step, and the columns a step is given are the step before’s', () => {
    expect(prepared.source).toEqual(held.run.source);
    expect(stepOf(prepared, 'sort').before).toEqual(held.run.source);
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
    expect(stepOf(prepared, 'sort').changes).toEqual([]);
    expect(stepOf(prepared, 'step_index').changes).toEqual([
      { name: 'step_index', change: 'added' },
    ]);
    expect(stepOf(prepared, 'join').changes).toEqual([{ name: 'sla_days', change: 'added' }]);
    expect(stepOf(prepared, 'pivot').changes).toEqual(
      ['Intake', 'Review', 'Pending', 'Complete'].map((name) => ({ name, change: 'added' })),
    );
  });

  it('say a column is replaced where the engine says so', () => {
    const variants = decodeRun(variantsFixture);
    const run = prepareRun(named(...variants.steps.map((step) => step.id)), variants);

    expect(stepOf(run, 'variants').changes).toEqual([
      { name: 'a', change: 'replaced' },
      { name: 'b', change: 'added' },
    ]);
  });

  it('say which steps stay lazy, as the engine reports it', () => {
    expect(stepOf(prepared, 'join').lazy).toBe(true);
    expect(stepOf(prepared, 'pivot').lazy).toBe(false);
  });
});

describe('the rows each step kept and the first values it added', () => {
  it('are how many rows the step kept of those it received, where the engine counted them', () => {
    expect(stepOf(prepared, 'sort').rows).toEqual({ received: 7, kept: 7 });
    // Seven claim rows make two claims.
    expect(stepOf(prepared, 'pivot').rows).toEqual({ received: 7, kept: 2 });
  });

  it('are the first values of each column a step changed, as one line of the text the engine wrote', () => {
    // A null is a dash, and a column with more values than are shown ends in an ellipsis.
    expect(stepOf(prepared, 'step_index').first).toEqual(
      new Map([['step_index', '1, 2, 3, 4, …']]),
    );
    expect(stepOf(prepared, 'resolved_day').first).toEqual(
      new Map([['resolved_day', '—, 2026-03-02, 2026-03-03, 2026-03-05, …']]),
    );
    expect(stepOf(prepared, 'join').first).toEqual(new Map([['sla_days', '1, 1, 2, 3, …']]));
    // The two rows of the pivot are all there are: no ellipsis.
    expect(stepOf(prepared, 'pivot').first).toEqual(
      new Map([
        ['Intake', '0, 0'],
        ['Review', '1, 2'],
        ['Pending', '2, —'],
        ['Complete', '2, 2'],
      ]),
    );
  });

  it('are nothing for a step the engine did not count, or could not run on the rows', () => {
    const variants = decodeRun(variantsFixture);
    const run = prepareRun(named(...variants.steps.map((step) => step.id)), variants);

    expect(stepOf(run, 'unknown_table')).toMatchObject({ rows: null, rowsProblem: null });
    expect(stepOf(run, 'failed')).toMatchObject({ rows: null, first: new Map() });
    expect(stepOf(run, 'variants').rows).toEqual({ received: 9, kept: 6 });
  });
});

describe('a step the engine could not run on the rows', () => {
  it('is valid, and says what went wrong', () => {
    const variants = decodeRun(variantsFixture);
    const run = prepareRun(named(...variants.steps.map((step) => step.id)), variants);

    expect(stepOf(run, 'failed')).toMatchObject({
      status: 'valid',
      rowsProblem: 'Could not be run on the sample rows: no rows',
    });
  });

  it('is nothing for the steps that were counted', () => {
    for (const step of prepared.steps) {
      expect(step, step.id).toMatchObject({ rowsProblem: null });
    }
  });
});

describe('the dtype the editor shows after each link of a chain', () => {
  it('is the engine’s dtype after the start and after each method, with where each stands', () => {
    expect(stepOf(prepared, 'step_index').chains.get(pathKey(COLUMN))).toEqual({
      source: 'String',
      links: ['UInt32', 'UInt32'],
      // After a text, then after a count, for each row, twice.
      places: [
        selectAfter('String', 'many'),
        selectAfter('UInt32', 'many'),
        selectAfter('UInt32', 'many'),
      ],
    });
    expect(stepOf(prepared, 'resolved_day').chains.get(pathKey(COLUMN))).toMatchObject({
      source: 'Datetime(us)',
      links: ['Date'],
    });
  });

  it('is the dtype of the condition, for a chain that starts from a when', () => {
    const test: Path = [
      ...COLUMN,
      { kind: 'start' },
      parameterSegment('predicates'),
      { kind: 'item', index: 0 },
    ];

    expect(stepOf(prepared, 'a_days').chains.get(pathKey(test))).toMatchObject({
      source: 'String',
      links: ['Boolean'],
    });
  });

  it('is the dtype of the input a chain starts from, with the methods that follow it', () => {
    expect(stepOf(prepared, 'holiday_count').chains.get(pathKey(COLUMN))).toEqual({
      source: 'Date',
      links: ['UInt32'],
      // A list of dates gives many; its length is one count.
      places: [selectAfter('Date', 'many'), selectAfter('UInt32', 'one')],
    });
  });

  it('is nothing where the engine gives no dtype', () => {
    // `element` has no dtype outside the cell of a pivot.
    expect(
      stepOf(prepared, 'pivot').chains.get(pathKey([parameterSegment('aggregate_function')])),
    ).toMatchObject({ source: null, links: [null] });
  });

  it('holds the arguments of a method as chains of their own, by their ways', () => {
    const other: Path = [...COLUMN, linkSegment(0), parameterSegment('other')];

    expect(stepOf(prepared, 'days_open').chains.get(pathKey(other))).toMatchObject({
      source: 'Datetime(us)',
      links: [],
    });
  });
});

describe('a refusal', () => {
  // The claims sorted, then a sum of a text column, which Polars refuses, then a step never run.
  const run = prepareRun(named('sort', 'total', 'after'), decodeRun(refusedFixture));

  it('is shown on the way it marks: here, the method the engine refused', () => {
    expect(stepOf(run, 'total')).toMatchObject({ status: 'refused' });
    expect(stepOf(run, 'total').problemAt).toEqual([...COLUMN, linkSegment(0)]);
  });

  it('says what Polars said of it, as far as the first paragraph, which is before its plan', () => {
    expect(stepOf(run, 'total').problem).toBe('`sum` operation not supported for dtype `str`');
  });

  it('gives the step the columns it was given, and the next step is not run', () => {
    const total = stepOf(run, 'total');

    expect(total.after).toEqual(total.before);
    expect(stepOf(run, 'after')).toMatchObject({
      status: 'notRun',
      problem: 'Not run: step 2 was refused.',
      problemAt: null,
    });
  });

  it('leaves the steps before it as they were', () => {
    expect(stepOf(run, 'sort')).toMatchObject({ status: 'valid', problem: null });
  });

  it('keeps the dtype the chain had before the method that was refused', () => {
    expect(stepOf(run, 'total').chains.get(pathKey(COLUMN))).toMatchObject({
      source: 'String',
      links: [null],
    });
  });
});

describe('what the engine says of why it refused', () => {
  const variants = decodeRun(variantsFixture);
  const run = prepareRun(named(...variants.steps.map((step) => step.id)), variants);

  it('is put in words by the kind of cause', () => {
    expect(stepOf(run, 'unknown_table').problem).toBe('There is no table "nope".');
    expect(stepOf(run, 'unknown_input').problem).toBe('There is no input "none".');
    expect(stepOf(run, 'build').problem).toBe('a parameter is missing');
    expect(stepOf(run, 'rule').problem).toBe('the reason');
    expect(stepOf(run, 'rows_stage').problem).toBe('the reason');
  });
});

describe('a step with something still to be picked', () => {
  const run = prepareRun(named('sort', 'later', 'after'), decodeRun(incompleteFixture));

  it('is said to be incomplete, on the way to what is to be picked', () => {
    expect(run.steps.map((step) => step.status)).toEqual(['valid', 'incomplete', 'notRun']);
    expect(stepOf(run, 'later').problem).toBe('Something in this step is still to be picked.');
    expect(stepOf(run, 'later').problemAt).toEqual([
      ...COLUMN,
      linkSegment(0),
      parameterSegment('n'),
      { kind: 'start' },
    ]);
  });

  it('keeps the places of the chain before the hole, where the argument would stand', () => {
    // The shift stands after the timestamp, for each row.
    expect(stepOf(run, 'later').chains.get(pathKey(COLUMN))?.places[0]).toEqual(
      selectAfter('Datetime(us)', 'many'),
    );
  });

  it('gives the step the columns it was given', () => {
    expect(stepOf(run, 'later').after).toEqual(stepOf(run, 'later').before);
  });

  it('says why the step after it was not run', () => {
    expect(stepOf(run, 'after').problem).toBe('Not run: step 2 still has something to be picked.');
  });
});

describe('the summary the engine wrote of the steps', () => {
  it('is shown for each step the report is of', () => {
    expect(stepOf(prepared, 'sort').summary).toBe('by dcn, timestamp');
    expect(prepared.steps.every((step) => step.summary.length > 0)).toBe(true);
  });
});

describe('the arguments that are at their default', () => {
  it('are the ways the engine reported, by key, for the step or the method that holds them', () => {
    // The sample gives `shift` its default of 1, the only argument at its default in the steps.
    expect(stepOf(prepared, 'resolved_at').atDefault).toEqual(
      new Set([pathKey([...COLUMN, linkSegment(0), parameterSegment('n')])]),
    );
    expect(stepOf(prepared, 'sort').atDefault).toEqual(new Set());
  });
});

describe('the words for a step that has no rows', () => {
  const step = stepOf(prepared, 'sort');

  it('follow the kind of the failure, not its text', () => {
    expect(rowsFailure('noSuchStep', 'there is no step "x"', null)).toBe(
      'This step is not in the document yet.',
    );
    expect(rowsFailure('limit', 'a preview holds at most 1000 rows', null)).toBe(
      'There are more rows than a preview holds.',
    );
    expect(rowsFailure('stale', 'the document is at revision 3', null)).toBe(
      'The steps changed; this will be asked again.',
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
  const step = stepOf(prepared, 'sort');

  it('are those the engine gave for that step', () => {
    expect(rowsToShow({ step: 'sort', outcome: { ok: true, answer: rows } }, 'sort', step)).toEqual(
      { kind: 'rows', columns: rows.columns, rows: rows.rows },
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
    ).toEqual({ kind: 'message', text: 'the reason' });
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
