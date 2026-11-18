import { describe, expect, it } from 'vitest';

import type { CandidateRequest, FitsView, OfferPlaceView, OptionsView } from '@dageist/host';
import {
  decodeCatalogue,
  decodeFits,
  decodeOffers,
  decodeOptions,
  decodeReadings,
} from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import fitsArgumentFixture from '../../../host/fixtures/fits-argument.json';
import fitsStartFixture from '../../../host/fixtures/fits-start.json';
import offersConditionFixture from '../../../host/fixtures/offers-condition.json';
import offersStartFixture from '../../../host/fixtures/offers-start.json';
import offersValueFixture from '../../../host/fixtures/offers-value.json';
import optionsFixture from '../../../host/fixtures/options.json';
import typedDateFixture from '../../../host/fixtures/typed-date.json';
import typedExpressionFixture from '../../../host/fixtures/typed-expression.json';
import typedStartAndArgumentFixture from '../../../host/fixtures/typed-start-and-argument.json';
import typedUnreadFixture from '../../../host/fixtures/typed-unread.json';
import typedWholeFixture from '../../../host/fixtures/typed-whole.json';
import { replaced } from '../../../host/test-support/tamper';
import { indexCatalogue } from './catalogue';
import {
  candidateKey,
  chosenOptions,
  prepareFits,
  prepareOffers,
  prepareOptions,
  prepareReadings,
} from './offers';
import { NO_POSITIONS, placeRequest, positionsOf, startRequest, within } from './place';

// The catalogue and the answers the host writes from the engine, as the editor gets them.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const afterDatetime = decodeOffers(offersValueFixture);
const atStart = decodeOffers(offersStartFixture);
const afterThen = decodeOffers(offersConditionFixture);

describe('the methods the engine offers after a timestamp', () => {
  const families = prepareOffers(catalogue, afterDatetime, ['expression']);
  const offerNamed = (name: string) =>
    families.flatMap((family) => family.offers).find((offer) => offer.name === name);

  it('are grouped by family, the shortest group first, and those of a length in the catalogue order', () => {
    // The methods of each family: arithmetic and window one, rows and cumulative three,
    // aggregate four, temporal five and compare eight.
    expect(families.map((family) => [family.key, family.offers.length])).toEqual([
      ['arithmetic', 1],
      ['window', 1],
      ['rows', 3],
      ['cumulative', 3],
      ['aggregate', 4],
      ['temporal', 5],
      ['compare', 8],
    ]);
    expect(families[0]?.title).toBe('Arithmetic');
  });

  it('carry the hint of the catalogue and are offered in the engine’s order', () => {
    expect(
      families.find((family) => family.key === 'rows')?.offers.map((offer) => offer.name),
    ).toEqual(['first', 'last', 'shift']);
    expect(offerNamed('shift')?.hint).toBe('value n rows earlier');
  });

  it('are refused with the engine’s reason, which says who refused', () => {
    const refused = offerNamed('cum_sum');
    expect(refused?.stoppedBy).toBe('engine');
    expect(refused?.reason).toContain('`cum_sum` operation not supported for dtype');
    expect(offerNamed('dt.date')).toMatchObject({ reason: null, stoppedBy: null });
  });

  it('say whether a chain may end at each of them', () => {
    expect(offerNamed('dt.date')?.ends).toEqual({ kind: 'allowed' });
    // A call that is refused has no end to speak of.
    expect(offerNamed('sum')?.ends).toEqual({ kind: 'notKnown' });
  });

  it('give the argument a method starts with, only where the engine says it fits', () => {
    expect(offerNamed('shift')?.argument).toEqual({
      parameter: 'n',
      literal: { kind: 'integer', value: 1 },
    });
    expect(offerNamed('dt.date')?.argument).toBeNull();
    expect(offerNamed('sum')?.argument).toBeNull();
  });
});

describe('the calls the engine offers at the start of an expression', () => {
  it('are the functions and the links of a condition, each with who refused it', () => {
    const families = prepareOffers(catalogue, atStart, ['function', 'condition']);
    expect(families.map((family) => [family.key, family.offers.length])).toEqual([
      ['start', 3],
      ['condition', 7],
    ]);
    const start = families.find((family) => family.key === 'start')?.offers ?? [];
    // `element` stands only in a pivot's aggregate, by a rule the engine holds.
    const element = start.find((offer) => offer.name === 'element');
    expect(element?.stoppedBy).toBe('engine');
    expect(element?.reason).toBe("`polars.element` stands only in a pivot's aggregate");
    expect(start.find((offer) => offer.name === 'col')).toMatchObject({ reason: null });
  });

  it('cannot be a further case of a condition, which the editor does not draw', () => {
    const rows = prepareOffers(catalogue, afterThen, ['condition']).flatMap(
      (family) => family.offers,
    );
    const byName = (id: string) => rows.find((offer) => offer.id === id);
    // After `then`, `otherwise` ends the condition, which the editor draws, and a further `when`
    // goes on, which it does not.
    expect(byName('Then.otherwise')).toMatchObject({ reason: null, stoppedBy: null });
    expect(byName('Then.when')).toMatchObject({
      stoppedBy: 'editor',
      reason: 'the editor draws one case of a when; a further case follows otherwise',
    });
    expect(byName('When.then')?.stoppedBy).toBe('engine');
  });
});

const field = (dtype: string): CandidateRequest => ({ kind: 'field', dtype });
const input = (dtype: string, list = false): CandidateRequest => ({ kind: 'input', dtype, list });

describe('the candidates that fit a slot', () => {
  it('are each given the dtype the engine gave them, and whether a chain may end at them', () => {
    // The candidates are the ones the host's snapshot of the fits of a predicate was asked about.
    const boolean = field('Boolean');
    const number = field('Int64');
    const candidates: CandidateRequest[] = [
      boolean,
      number,
      input('Boolean'),
      input('Date', true),
      field('Categorical'),
    ];
    const fits = prepareFits(candidates, decodeFits(fitsStartFixture), true);
    const fit = (candidate: CandidateRequest) => fits.get(candidateKey(candidate));

    // A predicate is a Boolean: every candidate may stand to be continued, and a number may not
    // end it.
    expect(fit(boolean)).toEqual({ dtype: 'Boolean', reason: null, ends: { kind: 'allowed' } });
    expect(fit(number)).toMatchObject({ dtype: 'Int64', reason: null, ends: { kind: 'refused' } });
    // What the tables cover no class of is not refused: it is left to the run.
    expect(fit(field('Categorical'))).toEqual({ dtype: null, reason: null, ends: null });
  });

  it('are refused with the engine’s reason as the argument of a method, with no end to speak of', () => {
    const text = field('String');
    const candidates: CandidateRequest[] = [text, field('Int64'), input('String')];
    const fits = prepareFits(candidates, decodeFits(fitsArgumentFixture), false);
    const reasons = candidates.map((candidate) => fits.get(candidateKey(candidate))?.reason);

    // `offset_by` takes text: a field of whole numbers is no duration.
    expect(reasons.map((reason) => reason === null)).toEqual([true, false, true]);
    expect(reasons[1]).toContain('expected `String`');
    expect(fits.get(candidateKey(text))?.ends).toBeNull();
  });

  it('are left out where the engine answered for fewer candidates than were asked', () => {
    const none: FitsView = { fits: [] };
    expect(prepareFits([field('Int64')], none, true).size).toBe(0);
  });
});

describe('the options of an entry', () => {
  const options = prepareOptions(decodeOptions(optionsFixture));

  it('start with the first word that is not the default and that Polars allows', () => {
    // A choice whose default is `inner`: the next word is `left`.
    expect(options.get('how')).toMatchObject({ default: 'inner', starter: 'left', stuck: null });
    // Beside a cross join every flag is refused: Polars takes no keys, and no option of them.
    expect(options.get('coalesce')).toMatchObject({ default: 'unset', starter: null });
    expect(options.get('coalesce')?.stuck).toContain('cross join');
  });

  it('start with the word the engine says, not one the editor picks from the words', () => {
    // An option the engine says starts with `right`, though `left` is the first other word and
    // Polars allows it: the editor does not choose between the words itself.
    const view: OptionsView = {
      options: [
        {
          parameter: 'how',
          default: 'inner',
          starter: 'right',
          words: ['inner', 'left', 'right'].map((word) => ({ word, refusal: null })),
        },
        {
          parameter: 'validate',
          default: 'm:m',
          starter: null,
          words: ['m:m', 'm:1'].map((word) => ({ word, refusal: null })),
        },
      ],
    };

    const prepared = prepareOptions(view);

    expect(prepared.get('how')).toMatchObject({ starter: 'right', stuck: null });
    // Where the engine says there is none, there is none.
    expect(prepared.get('validate')?.starter).toBeNull();
  });

  it('cannot be added where Polars refuses every word but the default, and say why', () => {
    // A cross join takes no keys: every validation is refused, the default `m:m` as well.
    const validate = options.get('validate');
    expect(validate?.starter).toBeNull();
    expect(validate?.stuck).toContain('cross');
    expect(validate?.words.map((word) => word.word)).toEqual(['m:m', 'm:1', '1:m', '1:1']);
  });

  it('are the flags and choices a step sets, as the words of the engine’s option table', () => {
    const join = catalogue.entries.get('LazyFrame.join');
    if (join === undefined) throw new Error('no join');
    expect(
      chosenOptions(join, {
        coalesce: { kind: 'flag', value: true },
        how: { kind: 'word', value: 'cross' },
        suffix: { kind: 'text', value: '_r' },
        left_on: { kind: 'list', items: [] },
      }),
    ).toEqual([
      { parameter: 'how', word: 'cross' },
      { parameter: 'coalesce', word: 'true' },
    ]);
  });
});

describe('where a slot stands', () => {
  const place: OfferPlaceView = {
    context: { outer: 'select', nested: 'condition.value' },
    after: { kind: 'value', dtype: 'Int64', shape: 'one' },
  };

  it('is the positions that hold the expression, which the engine reads as a context', () => {
    expect(positionsOf(place.context)).toEqual([
      { kind: 'outer', name: 'select' },
      { kind: 'nested', name: 'condition.value' },
    ]);
    expect(positionsOf({ outer: 'sort', nested: null })).toEqual([{ kind: 'outer', name: 'sort' }]);
    expect(placeRequest(place)).toEqual({
      positions: [
        { kind: 'outer', name: 'select' },
        { kind: 'nested', name: 'condition.value' },
      ],
      after: { kind: 'value', dtype: 'Int64', shape: 'one' },
    });
  });

  it('goes one position deeper by a parameter inside the expression', () => {
    const outer = [{ kind: 'outer', name: 'filter' }] as const;
    expect(within(outer, { kind: 'nested', name: 'over.partition_by' })).toEqual([
      { kind: 'outer', name: 'filter' },
      { kind: 'nested', name: 'over.partition_by' },
    ]);
    // A parameter that no named position holds leaves it where it is.
    expect(within(outer, null)).toBe(outer);
    expect(within(NO_POSITIONS, null)).toEqual([]);
  });

  it('starts an expression with a call that follows nothing', () => {
    expect(startRequest([{ kind: 'outer', name: 'sort' }])).toEqual({
      positions: [{ kind: 'outer', name: 'sort' }],
      after: { kind: 'start' },
    });
  });
});

describe('how the engine read a typed text', () => {
  // `1.5` typed into the argument of `dt.offset_by`, where the engine allows text and not a number.
  const asArgument = decodeReadings(typedExpressionFixture);

  it('gives each reading the literal and the dtype the engine gave it', () => {
    const [number, text] = prepareReadings(asArgument, false);
    expect(number).toMatchObject({ kind: 'read', literal: { kind: 'float', value: 1.5 } });
    expect(number).toMatchObject({ dtype: 'Float64' });
    expect(text).toEqual({
      kind: 'read',
      literal: { kind: 'text', value: '1.5' },
      dtype: 'String',
      reason: null,
      ends: null,
    });
  });

  it('refuses a reading the tables refuse, with the engine’s reason', () => {
    const [number] = prepareReadings(asArgument, false);
    expect(number?.kind === 'read' ? number.reason : null).toContain('expected `String`');
  });

  it('says whether a chain may end at a reading only where it stands as the start of one', () => {
    const [, text] = prepareReadings(asArgument, true);
    expect(text).toMatchObject({ kind: 'read', ends: { kind: 'allowed' } });
  });

  it('says whether the chain a reading starts may end, not whether the call it is given to may', () => {
    // `1.5` typed where it is the start of a filter's predicate and the argument of `shift`: the
    // call may end a chain and gives a value for each row, and the number and the text may not
    // end a predicate.
    const readings = prepareReadings(decodeReadings(typedStartAndArgumentFixture), true);
    expect(readings).toHaveLength(2);
    for (const reading of readings) {
      expect(reading).toMatchObject({
        kind: 'read',
        reason: null,
        ends: { kind: 'refused', reason: "a filter's predicate must be Boolean" },
      });
    }
  });

  it('judges nothing of a reading where no slot was asked about', () => {
    expect(prepareReadings(decodeReadings(typedWholeFixture), true)).toEqual([
      {
        kind: 'read',
        literal: { kind: 'integer', value: 12 },
        dtype: 'Int32',
        reason: null,
        ends: null,
      },
    ]);
  });

  it('gives a date as the day the engine read, with the dtype it gave', () => {
    expect(prepareReadings(decodeReadings(typedDateFixture), false)).toEqual([
      {
        kind: 'read',
        literal: { kind: 'text', value: '2026-03-10' },
        dtype: 'Date',
        reason: null,
        ends: null,
      },
    ]);
  });

  it('gives the first line of why a text is not what it was typed into', () => {
    const [unread] = prepareReadings(decodeReadings(typedUnreadFixture), false);
    expect(unread).toEqual({
      kind: 'unread',
      reason:
        "unit: 'x' not supported; available units are: 'y', 'mo', 'q', 'w', 'd', 'h', 'm', 's', 'ms', 'us', 'ns'",
    });
    const longer = decodeReadings(
      replaced(typedUnreadFixture, ['readings', 0, 'reason'], 'not a date\nPolars says more here'),
    );
    expect(prepareReadings(longer, false)).toEqual([{ kind: 'unread', reason: 'not a date' }]);
  });

  it('has nothing to say of a text that reads as nothing', () => {
    expect(prepareReadings({ readings: [] }, false)).toEqual([]);
  });
});
