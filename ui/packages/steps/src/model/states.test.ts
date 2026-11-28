import { describe, expect, it } from 'vitest';

import type {
  CauseView,
  CountView,
  DocumentStepView,
  ErrorKind,
  OutcomeView,
  RunView,
  StepReportView,
} from '@dageist/host';
import { ERROR_KINDS, decodeRun } from '@dageist/host';

import incompleteFixture from '../../../host/fixtures/step-report-incomplete.json';
import refusedFixture from '../../../host/fixtures/step-report-refused.json';
import reportFixture from '../../../host/fixtures/step-report.json';
import variantsFixture from '../../../host/fixtures/step-report-variants.json';
import { prepareRun, rowsFailure, sayingOf } from './report';

// Every outcome and every state of the rows that the host can report of a step, built from the
// reports the host writes, and what the editor says of each.
// A state that leaves a person with nothing to read is a gap, and it is declared or the check fails.
const refused = decodeRun(refusedFixture);
const incomplete = decodeRun(incompleteFixture);
const variants = decodeRun(variantsFixture);
const documents = decodeRun(reportFixture);

/** Steps with these names, which is all a report of them needs of the document. */
function named(run: RunView): DocumentStepView[] {
  return run.steps.map((step) => ({
    id: step.id,
    call: { entry: 'LazyFrame.select', arguments: [] },
    links: [],
  }));
}

/** The report of the step called `id` of `run`. */
function reportOf(run: RunView, id: string): StepReportView {
  const found = run.steps.find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found;
}

/** One state a step can be reported in. */
interface State {
  readonly name: string;
  /** The steps of the run the state stands in, the last of which is the step in the state. */
  readonly steps: readonly StepReportView[];
  readonly outcome: OutcomeView['status'];
  readonly cause: CauseView['kind'] | null;
  readonly count: CountView['kind'];
  /** What the editor says of the step beside its row. */
  readonly says: 'rows' | 'problem' | 'nothing';
}

/** The step `id` of `run` as the first of the steps of a state. */
function alone(run: RunView, id: string): readonly StepReportView[] {
  return [reportOf(run, id)];
}

/** A step Polars accepts, copied from `from`, that was not counted, called `id`. */
function uncounted(from: StepReportView, id: string): StepReportView {
  return { ...from, id, count: { kind: 'notCounted' } };
}

/** Every state, by what the host reports. The expectations are what the editor draws of each. */
const STATES: readonly State[] = [
  {
    name: 'accepted and counted',
    steps: alone(refused, 'sort'),
    outcome: 'valid',
    cause: null,
    count: 'counted',
    says: 'rows',
  },
  {
    name: 'accepted, counted, and collected the rows to be built',
    steps: alone(documents, 'pivot'),
    outcome: 'valid',
    cause: null,
    count: 'counted',
    says: 'rows',
  },
  {
    name: 'refused by the tables, with the words they recorded of Polars',
    steps: alone(refused, 'total'),
    outcome: 'refused',
    cause: 'notAllowed',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused by Polars where the tables allowed the step',
    steps: alone(variants, 'polars'),
    outcome: 'refused',
    cause: 'polars',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused for a table that is not connected',
    steps: alone(variants, 'unknown_table'),
    outcome: 'refused',
    cause: 'unknownTable',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused for an input that is not a connected value or list',
    steps: alone(variants, 'unknown_input'),
    outcome: 'refused',
    cause: 'unknownInput',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused because the catalogue could not build the call',
    steps: alone(variants, 'build'),
    outcome: 'refused',
    cause: 'build',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused by a rule the engine holds',
    steps: alone(variants, 'rule'),
    outcome: 'refused',
    cause: 'notAllowed',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'refused by a rule where the rows would have been read',
    steps: alone(variants, 'rows_stage'),
    outcome: 'refused',
    cause: 'notAllowed',
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'with something still to be picked',
    steps: alone(incomplete, 'later'),
    outcome: 'incomplete',
    cause: null,
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'not run after a step that was refused',
    steps: refused.steps,
    outcome: 'notRun',
    cause: null,
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'not run after a step with something still to be picked',
    steps: incomplete.steps,
    outcome: 'notRun',
    cause: null,
    count: 'notCounted',
    says: 'problem',
  },
  {
    name: 'accepted and failed on the rows',
    steps: alone(variants, 'failed'),
    outcome: 'valid',
    cause: null,
    count: 'failed',
    says: 'problem',
  },
  {
    name: 'accepted and not run on the rows after a step that failed there',
    steps: [reportOf(variants, 'failed'), reportOf(variants, 'not_reached')],
    outcome: 'valid',
    cause: null,
    count: 'notReached',
    says: 'problem',
  },
];

/** The run a state stands in, which is built from the steps that make it up. */
function runOf(state: State): RunView {
  return { source: documents.source, steps: state.steps };
}

/** What the editor says of the last step of the state `state`. */
function sayingAt(state: State): ReturnType<typeof sayingOf> {
  const run = runOf(state);
  const last = prepareRun(named(run), run).steps.at(-1);
  if (last === undefined) throw new Error(`${state.name} has no step`);
  return sayingOf(last);
}

/** Returns the names of the states of `states` that say nothing, though the step is one to say something of. */
function silent(states: readonly State[] = STATES): string[] {
  return states.filter((state) => sayingAt(state).says === 'nothing').map((state) => state.name);
}

describe('the states a step can be reported in', () => {
  it('hold every outcome, every cause of a refusal and every state of the rows the host can report', () => {
    // A variant the host adds to one of these fails to compile here, until a state holds it.
    const outcomes = {
      valid: 0,
      refused: 0,
      incomplete: 0,
      notRun: 0,
    } satisfies Record<OutcomeView['status'], number>;
    const causes = {
      unknownTable: 0,
      unknownInput: 0,
      build: 0,
      notAllowed: 0,
      polars: 0,
    } satisfies Record<CauseView['kind'], number>;
    const counts = { notCounted: 0, notReached: 0, counted: 0, failed: 0 } satisfies Record<
      CountView['kind'],
      number
    >;
    for (const state of STATES) {
      outcomes[state.outcome] += 1;
      if (state.cause !== null) causes[state.cause] += 1;
      counts[state.count] += 1;
    }
    for (const [name, held] of [
      ...Object.entries(outcomes),
      ...Object.entries(causes),
      ...Object.entries(counts),
    ]) {
      expect(held, name).toBeGreaterThan(0);
    }
  });

  it('are what the host reports of the steps the states are built from', () => {
    for (const state of STATES) {
      const last = state.steps.at(-1);
      expect(last, state.name).toBeDefined();
      if (last === undefined) continue;
      expect(last.outcome.status, state.name).toBe(state.outcome);
      expect(last.outcome.status === 'refused' ? last.outcome.cause.kind : null, state.name).toBe(
        state.cause,
      );
      expect(last.count.kind, state.name).toBe(state.count);
    }
  });
});

describe('what the editor says of a step in each state', () => {
  it('is the rows for a step the engine counted, words for what is the matter, and nothing where the step was not counted', () => {
    for (const state of STATES) {
      expect(sayingAt(state).says, state.name).toBe(state.says);
    }
  });

  it('is words, one line or more, for every state where something is the matter', () => {
    for (const state of STATES.filter((held) => held.says === 'problem')) {
      const saying = sayingAt(state);
      expect(saying.says, state.name).toBe('problem');
      if (saying.says !== 'problem') continue;
      for (const line of saying.lines) {
        expect(line.trim().length, state.name).toBeGreaterThan(0);
      }
    }
  });

  it('is never nothing for a state the host reports, so none is declared as a gap', () => {
    expect(silent()).toEqual([]);
  });

  it('is found where a state says nothing, such as a step accepted whose rows were not read', () => {
    const quiet: State = {
      name: 'accepted and not counted, with no rows read',
      steps: [uncounted(reportOf(refused, 'sort'), 'quiet')],
      outcome: 'valid',
      cause: null,
      count: 'notCounted',
      says: 'nothing',
    };

    expect(silent([...STATES, quiet])).toEqual([quiet.name]);
  });

  it('is said of a step after one that failed on the rows as what the step before it said: a matter, and which step failed', () => {
    const state = STATES.find((held) => held.count === 'notReached');
    expect(state).toBeDefined();
    if (state === undefined) return;
    const run = runOf(state);
    const prepared = prepareRun(named(run), run);
    expect(prepared.steps.map((step) => sayingOf(step))).toEqual([
      { says: 'problem', lines: ['Could not be run on the sample rows: no rows'] },
      { says: 'problem', lines: ['Not run on the sample rows: step 1 failed on them.'] },
    ]);
  });
});

describe('what the rows list says when the host gives no rows', () => {
  it('is words for every kind of failure the host reports', () => {
    const words = {
      noSuchStep: 'This step is not in the document yet.',
      notRun: 'Not run: a step before it was refused.',
      refused: 'This step was refused.',
      limit: 'There are more rows than a preview holds.',
      stale: 'The steps changed; this will be asked again.',
      unknownName: 'the host’s own words',
      failed: 'the host’s own words',
    } satisfies Record<ErrorKind, string>;
    for (const kind of ERROR_KINDS) {
      expect(rowsFailure(kind, 'the host’s own words', null), kind).toBe(words[kind]);
    }
  });
});
