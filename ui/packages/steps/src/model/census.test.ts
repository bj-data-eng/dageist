import { describe, expect, it } from 'vitest';

import { DecodeError } from '@dageist/host';

import censusFixture from '../../../host/fixtures/census.json';
import { decodeCensus } from '../../../host/test-support/census';
import type { Census } from '../../../host/test-support/census';
import { compareCensus, planFor } from '../../test-support/census-plans';
import { sorted } from '../../test-support/sorted';
import { GAPS, coverKey } from './limits';

// Every variant the step editor has to handle, as the host writes them from the Rust types, held
// against what the editor can draw. A variant the engine or the host gains reaches the census, and
// from it this test, so it fails until the editor draws the variant or declares it a gap with its
// reason. The expectations are written from what the editor does and what each list means, not from
// what the run prints.
const census = decodeCensus(censusFixture);
const planned = planFor(census);

/** The names of the lists of a census. */
type ListName = {
  [Name in keyof Census]: Census[Name] extends readonly unknown[] ? Name : never;
}[keyof Census];

/** The census with `extra` added to its list `name`. */
function plus<Name extends ListName>(
  name: Name,
  extra: Census[Name] extends ReadonlyArray<infer Item> ? Item : never,
): Census {
  const held: readonly unknown[] = census[name];
  return Object.assign({}, census, { [name]: [...held, extra] });
}

describe('the census the host wrote', () => {
  it('lists the kinds of start, of parameter and of option, the classes of dtype, the causes and rules of a refusal, the states of a step and the kinds of input and of value', () => {
    // Each kind of start; a field, a connected value and list, a hole, four kinds of literal typed in.
    const starts = new Set(census.starts.map((start) => start.kind));
    expect(starts).toEqual(
      new Set(['field', 'connectedValue', 'connectedList', 'hole', 'typed', 'function']),
    );
    expect(census.typedValues.map((value) => value.literal)).toEqual([
      'integer',
      'float',
      'text',
      'flag',
    ]);
    // The twelve kinds of parameter the contract names, of which two are set as options.
    expect(census.parameterKinds).toHaveLength(13);
    expect(census.optionKinds).toEqual(['flag', 'choice']);
    // The classes of dtype the tables hold lines for and the lists and arrays of the others, in three shapes.
    expect(census.dtypeClasses).toHaveLength(67);
    expect(census.valueShapes).toEqual(['one', 'many', 'counted']);
    expect(census.causes).toHaveLength(5);
    expect(census.outcomes).toEqual(['valid', 'refused', 'incomplete', 'notRun']);
    expect(census.rowsStates).toEqual(['notCounted', 'notReached', 'counted', 'failed']);
    expect(census.inputKinds).toEqual(['table', 'value', 'list']);
    expect(census.cells).toEqual(['text', 'null']);
  });

  it('holds a state of a step after one that was refused and after one that failed on the rows', () => {
    const after = new Set(census.states.map((state) => state.after));
    expect(after).toEqual(new Set(['nothing', 'refused', 'incomplete', 'failed', 'notRun']));
    expect(
      census.states.some(
        (state) =>
          state.outcome === 'valid' && state.rows === 'notReached' && state.after === 'failed',
      ),
    ).toBe(true);
    // A step Polars accepts is not left uncounted after a failure: it is not reached.
    expect(
      census.states.some(
        (state) =>
          state.outcome === 'valid' && state.rows === 'notCounted' && state.after === 'failed',
      ),
    ).toBe(false);
  });

  it('has a plan in the editor for every variant it lists', () => {
    expect(planned.unplanned).toEqual([]);
  });

  it('shows the gaps that are declared, and no gap that is not declared', () => {
    expect(compareCensus(planned, GAPS)).toEqual({ undeclared: [], stale: [] });
  });

  it('shows the gaps of a flag a document holds typed in, which the host cannot type and the editor does not show', () => {
    // The document holds a number, text or a flag as the value of `lit`; the host reads typed text as
    // a number or text, and the literal control shows a number or text. A date, a time, a moment or
    // a length of time is not held by a document.
    expect(sorted([...planned.gaps.keys()])).toEqual([
      'shown literal flag',
      'shown resolution several',
      'typed flag',
    ]);
    expect(
      census.typedValues.filter((value) => !value.readable).map((value) => value.literal),
    ).toEqual(['flag']);
  });

  it('lists the families the view decodes and decides on', () => {
    expect(census.changes).toEqual(['added', 'replaced']);
    expect(census.resolutions).toEqual(['dtype', 'several', 'unfinished', 'noContext', 'refused']);
    expect(census.editAnswers).toEqual(['edited', 'refused']);
    expect(census.problemKinds).toHaveLength(13);
    expect(census.valueKinds).toHaveLength(16);
    expect(census.settingKinds).toHaveLength(11);
    expect(census.defaultKinds).toHaveLength(8);
  });

  it('reads the typed text of a date and of a duration where a slot takes one, and no other moment', () => {
    // A date is read as the text of the day with the class Date, in a slot that takes a date; a
    // duration has no class. Neither is a start: an expression starts from a number or text.
    const slots = census.typedReadings.map((reading) => reading.slot.kind);
    expect(slots).toContain('duration');
    expect(census.typedReadings).toContainEqual({
      slot: { kind: 'value', type: 'date' },
      literal: 'text',
      dtype: 'Date',
    });
    expect(census.starts.filter((start) => start.kind === 'typed')).toHaveLength(4);
  });
});

describe('a variant the census gains that the editor lacks', () => {
  it('is refused by the host’s decoder where it is a kind of start the contract does not name', () => {
    const odd = { ...census, starts: [...census.starts, { kind: 'dateTime' }] };

    expect(() => decodeCensus(odd)).toThrow(DecodeError);
  });

  it('fails the check where it is a rule the decoders refuse, and where it is a rule with no words', () => {
    expect(
      planFor(plus('rules', { name: 'a-new-rule', statement: 'says something' })).unplanned,
    ).toEqual([expect.stringContaining('refusal by rule a-new-rule')]);
    expect(
      planFor(plus('rules', { name: 'pattern-is-valid', statement: ' ' })).unplanned,
    ).toContain('rule pattern-is-valid: says nothing');
  });

  it('fails the check where it is a reason the walk cannot tell that says nothing', () => {
    expect(
      planFor(plus('notKnown', { name: 'a-new-reason', because: ' ' })).unplanned,
    ).toEqual(['not known a-new-reason: says nothing']);
  });

  it('fails the check where it is a class of dtype, a shape or a state of a condition the grid has no place for', () => {
    expect(planFor(plus('dtypeClasses', 'Decimal(38, 9)')).unplanned).toEqual([
      'dtype class Decimal(38, 9): not in the grid',
    ]);
    expect(planFor(plus('valueShapes', 'few')).unplanned).toEqual([
      'value shape few: not in the grid',
    ]);
    expect(planFor(plus('conditionStates', 'otherwise')).unplanned).toEqual([
      'state of a condition otherwise: not in the grid',
    ]);
  });

  it('fails the check where it is a function that starts a chain and the grid test runs none of that name', () => {
    expect(
      planFor(plus('starts', { kind: 'function', entry: 'polars.a_new_start' })).unplanned,
    ).toEqual(['start function polars.a_new_start']);
  });

  it('fails the check where it is a typed value of a kind the grid test has no start for, such as a date', () => {
    const withDate: Census = {
      ...plus('typedValues', { literal: 'date', dtype: 'Date', readable: false }),
      starts: [...census.starts, { kind: 'typed', literal: 'date' }],
    };
    const run = planFor(withDate);

    expect(run.unplanned).toEqual([
      'start typed date',
      'typed value date',
      'typed value date: no argument',
    ]);
    // It is a gap too, since the host reads no typed text as one, and the check fails until it is declared.
    expect(compareCensus(run, GAPS)).toEqual({ undeclared: ['typed date'], stale: [] });
  });

  it('fails the check where it is a kind of parameter, an option or a shape the controls do not draw', () => {
    expect(planFor(plus('parameterKinds', 'matrix')).unplanned).toEqual(['parameter kind matrix']);
    expect(planFor(plus('optionKinds', 'matrix')).unplanned).toEqual([
      'option kind matrix: not a kind of parameter',
    ]);
    expect(planFor(plus('parameterShapes', 'several')).unplanned).toEqual([
      'parameter shape several',
    ]);
  });

  it('fails the check where it is an outcome, a rows state or a state of a step the editor has no words for', () => {
    expect(
      planFor(
        plus('states', { outcome: 'paused', cause: null, rows: 'notCounted', after: 'nothing' }),
      ).unplanned,
    ).toEqual([expect.stringContaining('state paused')]);
    expect(
      planFor(plus('states', { outcome: 'valid', cause: null, rows: 'partly', after: 'nothing' }))
        .unplanned,
    ).toEqual([expect.stringContaining('state valid')]);
  });

  it('fails the check where it is a kind of input, an origin or a cell the editor does not show', () => {
    expect(planFor(plus('inputKinds', 'stream')).unplanned).toHaveLength(census.origins.length);
    expect(planFor(plus('origins', 'sensor')).unplanned).toHaveLength(census.inputKinds.length);
    expect(planFor(plus('origins', 'sensor')).unplanned).toEqual(
      sorted(census.inputKinds.map((kind) => `input ${kind} from sensor`)),
    );
    expect(planFor(plus('cells', 'error')).unplanned).toEqual(['cell error']);
  });
});

describe('a family of variants the census gains that the editor lacks', () => {
  it('fails the check where it is a cause, a stage, a source or a rows state the decoders refuse', () => {
    expect(planFor(plus('causes', 'timedOut')).unplanned).toEqual(['cause timedOut']);
    expect(planFor(plus('stages', 'planning')).unplanned).toEqual([
      'refusal by polars at planning: $.steps[0].outcome.cause.refusal.source.stage: must be one of "schema", "rows", "empty", not "planning"',
    ]);
    expect(planFor(plus('refusalSources', 'network')).unplanned).toEqual([
      'refusal source network',
    ]);
    expect(planFor(plus('rowsStates', 'partly')).unplanned).toEqual(['rows state partly']);
    expect(planFor(plus('outcomes', 'paused')).unplanned).toEqual(['outcome paused']);
  });

  it('fails the check where it is a change, a resolution, a default or a setting the decoders refuse', () => {
    expect(planFor(plus('changes', 'dropped')).unplanned).toEqual([
      expect.stringContaining('change dropped'),
    ]);
    expect(planFor(plus('resolutions', 'guessed')).unplanned).toEqual(['resolution guessed']);
    expect(planFor(plus('defaultKinds', 'computed')).unplanned).toEqual(['default computed']);
    expect(planFor(plus('settingKinds', 'range')).unplanned).toEqual(['setting range']);
  });

  it('fails the check where it is a kind of edit answer, of refusal, of thing or of value the decoders refuse', () => {
    expect(planFor(plus('editAnswers', 'deferred')).unplanned).toEqual(['edit answer deferred']);
    expect(planFor(plus('problemKinds', 'timedOut')).unplanned).toEqual(['problem timedOut']);
    expect(planFor(plus('thingKinds', 'sheet')).unplanned).toEqual([
      expect.stringContaining('thing sheet'),
    ]);
    expect(planFor(plus('valueKinds', 'moment')).unplanned).toEqual([
      expect.stringContaining('value kind moment'),
    ]);
    expect(planFor(plus('wanted', 'sheet')).unplanned).toEqual([
      expect.stringContaining('wanted sheet'),
    ]);
  });

  it('shows the flag as not shown and an expression of several columns as one that has no dtype shown', () => {
    expect(planned.gaps.get('shown literal flag')).toEqual({ kind: 'shown', what: 'literal flag' });
    expect(planned.gaps.get('shown resolution several')).toEqual({
      kind: 'shown',
      what: 'resolution several',
    });
    // Nothing else the census holds is a value that the editor leaves unshown.
    expect([...planned.gaps.values()].filter((gap) => gap.kind === 'shown')).toHaveLength(2);
  });
});

describe('a gap that goes stale when the editor draws what it lacked', () => {
  it('fails the check where the editor shows the dtypes of an expression that gives several columns', () => {
    // An editor that shows a dtype wherever the engine gives one, as it would for several columns.
    const run = planFor(census, () => 'Int64');

    const { undeclared, stale } = compareCensus(run, GAPS);

    expect(undeclared).toEqual([]);
    expect(stale).toContain('shown resolution several');
  });
});

describe('a gap that is declared and the census does not show', () => {
  it('fails the check where no flag is a typed value of the census, so that the declared gap is stale', () => {
    const without: Census = {
      ...census,
      typedValues: census.typedValues.filter((value) => value.literal !== 'flag'),
      starts: census.starts.filter(
        (start) => !(start.kind === 'typed' && start.literal === 'flag'),
      ),
    };
    const run = planFor(without);

    expect(compareCensus(run, GAPS)).toEqual({
      undeclared: [],
      stale: ['shown literal flag', 'typed flag'],
    });
  });

  it('declares the flag once, with its reason', () => {
    const flags = GAPS.filter((gap) => coverKey(gap.cover) === 'typed flag');
    expect(flags).toHaveLength(1);
    expect(flags[0]?.reason).toContain('never as a flag');
  });
});
