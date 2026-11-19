import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import {
  decodeFits,
  decodeOffers,
  decodeOptions,
  decodeReadings,
  decodeStepStarts,
} from './decode-offers';
import fitsArgumentFixture from '../fixtures/fits-argument.json';
import fitsStartFixture from '../fixtures/fits-start.json';
import offersConditionFixture from '../fixtures/offers-condition.json';
import offersStartFixture from '../fixtures/offers-start.json';
import offersValueFixture from '../fixtures/offers-value.json';
import optionsFixture from '../fixtures/options.json';
import stepStartsFixture from '../fixtures/step-starts.json';
import typedDateFixture from '../fixtures/typed-date.json';
import typedExpressionFixture from '../fixtures/typed-expression.json';
import typedUnreadFixture from '../fixtures/typed-unread.json';
import typedWholeFixture from '../fixtures/typed-whole.json';
import { replaced, withField, without } from '../test-support/tamper';

// The fixtures are written from Rust by the host's snapshot test. They are read as `unknown`,
// as the host's answer is, and the decoder is what says what they are.
const offersStart: unknown = offersStartFixture;
const offersValue: unknown = offersValueFixture;
const offersCondition: unknown = offersConditionFixture;
const fitsStart: unknown = fitsStartFixture;
const fitsArgument: unknown = fitsArgumentFixture;
const options: unknown = optionsFixture;
const stepStarts: unknown = stepStartsFixture;
const typedExpression: unknown = typedExpressionFixture;
const typedWhole: unknown = typedWholeFixture;
const typedDate: unknown = typedDateFixture;
const typedUnread: unknown = typedUnreadFixture;

/** What decoding `value` with `decode` was refused with. */
function refusal(decode: (value: unknown) => unknown, value: unknown): DecodeError {
  try {
    decode(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

/** Where in the entries of `value` the entry `id` is. */
function entryAt(value: unknown, id: string): number {
  const found = decodeOffers(value).entries.findIndex((entry) => entry.entry === id);
  if (found < 0) throw new Error(`no entry ${id}`);
  return found;
}

describe('the offers at a place', () => {
  it('say the place they were asked at, as the engine took it', () => {
    expect(decodeOffers(offersValue).place).toEqual({
      context: { outer: 'select', nested: null },
      after: { kind: 'value', dtype: 'Datetime(us)', shape: 'many' },
    });
    expect(decodeOffers(offersStart).place.after).toEqual({ kind: 'start' });
  });

  it('offer every entry that stands in an expression, and no step', () => {
    const entries = decodeOffers(offersValue).entries;
    expect(entries).toHaveLength(47);
    expect(entries.some((entry) => entry.entry.startsWith('LazyFrame.'))).toBe(false);
  });

  it('give what a call that may stand gives, and whether the chain may end there', () => {
    const entries = decodeOffers(offersValue).entries;
    // The date of a timestamp is a date, one for each row, and the chain may end there.
    expect(entries.find((entry) => entry.entry === 'Expr.dt.date')?.verdict).toEqual({
      kind: 'allowed',
      result: 'Date',
      shape: 'many',
      ending: { kind: 'allowed' },
    });
    // The first of the timestamps is one timestamp.
    expect(entries.find((entry) => entry.entry === 'Expr.first')?.verdict).toMatchObject({
      kind: 'allowed',
      result: 'Datetime(us)',
      shape: 'one',
    });
  });

  it('give the reason a call may not stand, and who refused it', () => {
    const sum = decodeOffers(offersValue).entries.find((entry) => entry.entry === 'Expr.sum');
    expect(sum?.verdict).toMatchObject({
      kind: 'refused',
      refusal: { source: { by: 'polars', stage: 'rows' } },
    });
    expect(sum?.verdict.kind === 'refused' ? sum.verdict.refusal.reason : null).toContain(
      'datetime',
    );
    const element = decodeOffers(offersStart).entries.find(
      (entry) => entry.entry === 'polars.element',
    );
    expect(element?.verdict).toMatchObject({
      kind: 'refused',
      refusal: { source: { by: 'rule', rule: 'element-only-in-pivot-aggregate' } },
    });
  });

  it('give the argument a method starts with where the tables allow it', () => {
    const entries = decodeOffers(offersValue).entries;
    expect(entries.find((entry) => entry.entry === 'Expr.shift')?.argument).toEqual({
      parameter: 'n',
      literal: { kind: 'integer', value: 1, shown: '1' },
    });
    expect(entries.find((entry) => entry.entry === 'Expr.dt.date')?.argument).toBeNull();
  });

  it('continue an unfinished condition with the links that follow it', () => {
    const allowed = decodeOffers(offersCondition)
      .entries.filter((entry) => entry.verdict.kind === 'allowed')
      .map((entry) => entry.entry);
    expect(allowed).toEqual(['Then.otherwise', 'Then.when']);
  });

  it('are refused with the place of what is wrong', () => {
    const at = entryAt(offersValue, 'Expr.shift');
    expect(
      refusal(
        decodeOffers,
        replaced(offersValue, ['entries', at, 'argument', 'literal', 'kind'], 'big'),
      ).message,
    ).toBe(
      `$.entries[${String(at)}].argument.literal.kind: must be one of "integer", "float", "text", not "big"`,
    );
    expect(
      refusal(decodeOffers, without(offersValue, ['entries', at, 'argument', 'literal'], 'shown'))
        .message,
    ).toBe(`$.entries[${String(at)}].argument.literal: is missing the field "shown"`);
    expect(refusal(decodeOffers, without(offersValue, ['entries', at], 'verdict')).message).toBe(
      `$.entries[${String(at)}]: is missing the field "verdict"`,
    );
    expect(refusal(decodeOffers, withField(offersValue, [], 'extra', 1)).message).toBe(
      '$: has the field "extra", which the contract lacks',
    );
    expect(
      refusal(decodeOffers, replaced(offersValue, ['place', 'context', 'outer'], 'selects')).path,
    ).toBe('$.place.context.outer');
  });
});

describe('the fits of candidates for a slot', () => {
  it('say the dtype the engine gave each candidate and whether it fits', () => {
    const fits = decodeFits(fitsStart).fits;
    expect(fits).toHaveLength(5);
    // Candidates are asked about in the order they were sent: a Boolean field, a whole number
    // field, a Boolean value, a list of dates, and a dtype the tables cover no class of.
    const dtypes = fits.map((fit) => (fit.kind === 'judged' ? fit.dtype : null));
    expect(dtypes).toEqual(['Boolean', 'Int64', 'Boolean', 'Date', null]);
  });

  it('say whether a candidate may end where it stands', () => {
    const [boolean, number] = decodeFits(fitsStart).fits;
    expect(boolean).toMatchObject({
      kind: 'judged',
      verdict: { kind: 'allowed', ending: { kind: 'allowed' } },
    });
    expect(number).toMatchObject({
      kind: 'judged',
      verdict: {
        kind: 'allowed',
        ending: {
          kind: 'refused',
          refusal: { source: { by: 'rule', rule: 'filter-predicate-is-boolean' } },
        },
      },
    });
  });

  it('say that a candidate the tables cover no class of is not judged', () => {
    expect(decodeFits(fitsStart).fits[4]).toEqual({
      kind: 'unjudged',
      reason: 'the tables cover no class of the dtype Categorical',
    });
  });

  it('refuse a candidate that does not fit the argument, with the engine reason', () => {
    const kinds = decodeFits(fitsArgument).fits.map((fit) =>
      fit.kind === 'judged' ? fit.verdict.kind : fit.kind,
    );
    // `dt.offset_by` takes a duration as text: a field of text and a connected text fit; a whole
    // number field does not.
    expect(kinds).toEqual(['allowed', 'refused', 'allowed']);
  });

  it('are refused with the place of what is wrong', () => {
    expect(refusal(decodeFits, replaced(fitsStart, ['fits', 0, 'kind'], 'maybe')).message).toBe(
      '$.fits[0].kind: must be one of "judged", "unjudged", not "maybe"',
    );
    expect(refusal(decodeFits, without(fitsStart, ['fits', 4], 'reason')).message).toBe(
      '$.fits[4]: is missing the field "reason"',
    );
  });
});

describe('the readings of a typed text', () => {
  it('give each literal the text reads as, with the dtype the engine gave it', () => {
    // `1.5` typed into the argument of `dt.offset_by`: a number with a fraction, and the text.
    const [number, text] = decodeReadings(typedExpression).readings;
    expect(number).toMatchObject({
      kind: 'read',
      literal: { kind: 'float', value: 1.5, shown: '1.5' },
      dtype: 'Float64',
    });
    expect(text).toMatchObject({
      kind: 'read',
      literal: { kind: 'text', value: '1.5', shown: '1.5' },
      dtype: 'String',
    });
  });

  it('say what the tables allow of each reading in the slot that was asked about', () => {
    // `dt.offset_by` takes a duration as text: the number is refused when Polars runs rows.
    const [number, text] = decodeReadings(typedExpression).readings;
    expect(number).toMatchObject({
      fit: {
        kind: 'judged',
        dtype: 'Float64',
        verdict: { kind: 'refused', refusal: { source: { by: 'polars', stage: 'rows' } } },
      },
    });
    expect(text).toMatchObject({
      fit: { kind: 'judged', dtype: 'String', verdict: { kind: 'allowed' } },
    });
  });

  it('say nothing of the slot where none was asked about', () => {
    expect(decodeReadings(typedWhole).readings).toEqual([
      {
        kind: 'read',
        literal: { kind: 'integer', value: 12, shown: '12' },
        dtype: 'Int32',
        fit: null,
      },
    ]);
  });

  it('give a date as the day Polars read, which has the dtype Date', () => {
    // `10-03-2026` is a day, month and year; the day is carried as its ISO text.
    expect(decodeReadings(typedDate).readings).toEqual([
      {
        kind: 'read',
        literal: { kind: 'text', value: '2026-03-10', shown: '2026-03-10' },
        dtype: 'Date',
        fit: null,
      },
    ]);
  });

  it('give the reason a text is not what it was typed into', () => {
    const [reading] = decodeReadings(typedUnread).readings;
    expect(reading?.kind).toBe('unread');
    expect(reading?.kind === 'unread' ? reading.reason : '').toContain('unit');
  });

  it('are refused with the place of what is wrong', () => {
    expect(
      refusal(decodeReadings, replaced(typedExpression, ['readings', 0, 'kind'], 'maybe')).message,
    ).toBe('$.readings[0].kind: must be one of "read", "unread", not "maybe"');
    expect(refusal(decodeReadings, without(typedWhole, ['readings', 0], 'dtype')).message).toBe(
      '$.readings[0]: is missing the field "dtype"',
    );
    expect(
      refusal(decodeReadings, replaced(typedWhole, ['readings', 0, 'literal', 'value'], 1.5))
        .message,
    ).toBe('$.readings[0].literal.value: must be a whole number, not 1.5');
    expect(refusal(decodeReadings, withField(typedUnread, [], 'extra', 1)).message).toBe(
      '$: has the field "extra", which the contract lacks',
    );
  });
});

describe('the options of an entry', () => {
  it('give each flag and choice with its default and the words it may be set to', () => {
    const decoded = decodeOptions(options).options;
    const named = (name: string) => decoded.find((option) => option.parameter === name);
    expect(named('how')?.default).toBe('inner');
    expect(named('validate')?.words.map((word) => word.word)).toEqual(['m:m', 'm:1', '1:m', '1:1']);
    expect(named('coalesce')).toMatchObject({ default: 'unset' });
    expect(named('coalesce')?.words.map((word) => word.word)).toEqual(['true', 'false']);
  });

  it('name the word an option starts with when it is added, where Polars allows another than its default', () => {
    const decoded = decodeOptions(options).options;
    const starter = (name: string) => decoded.find((option) => option.parameter === name)?.starter;
    // Beside a cross join every validation but the default is refused, so there is none; `how`
    // is judged against the others, and `left` is the first word after its default `inner`.
    expect(starter('validate')).toBeNull();
    expect(starter('how')).toBe('left');
  });

  it('say why an option cannot be added, with the refusal that stops it, where it has no starter', () => {
    const decoded = decodeOptions(options).options;
    const named = (name: string) => decoded.find((option) => option.parameter === name);
    expect(named('validate')?.stuck?.source).toEqual({ by: 'polars', stage: 'schema' });
    expect(named('how')?.stuck).toBeNull();
  });

  it('give the reason Polars refuses a word beside the options already chosen', () => {
    // A cross join takes no join keys, so every validation is refused with Polars' reason.
    const validate = decodeOptions(options).options.find(
      (option) => option.parameter === 'validate',
    );
    for (const word of validate?.words ?? []) {
      expect(word.refusal?.source).toEqual({ by: 'polars', stage: 'schema' });
    }
    const how = decodeOptions(options).options.find((option) => option.parameter === 'how');
    expect(how?.words.every((word) => word.refusal === null)).toBe(true);
  });

  it('are refused with the place of what is wrong', () => {
    expect(refusal(decodeOptions, replaced(options, ['options', 0, 'words'], 'none')).message).toBe(
      '$.options[0].words: must be a list, not a string',
    );
    expect(refusal(decodeOptions, withField(options, ['options', 0], 'extra', 1)).message).toBe(
      '$.options[0]: has the field "extra", which the contract lacks',
    );
  });
});

describe('what each step starts with', () => {
  const starts = decodeStepStarts(stepStarts).starts;
  const startOf = (entry: string) => {
    const found = starts.find((start) => start.entry === entry);
    if (found === undefined) throw new Error(`no start for ${entry}`);
    return found;
  };

  it('has one for each step of the list, ordered by identifier', () => {
    expect(starts.map((start) => start.entry)).toEqual([
      'LazyFrame.filter',
      'LazyFrame.join',
      'LazyFrame.pivot',
      'LazyFrame.rename',
      'LazyFrame.select',
      'LazyFrame.sort',
      'LazyFrame.with_columns',
    ]);
  });

  it('gives a select every column of the table in its order, as fields', () => {
    expect(startOf('LazyFrame.select').arguments).toEqual([
      {
        parameter: 'exprs',
        setting: {
          kind: 'list',
          items: ['dcn', 'timestamp', 'step_to', 'owner', 'row_days'].map((column) => ({
            kind: 'field',
            column,
          })),
        },
      },
    ]);
  });

  it('gives a join the first connected table, keys still to be given, and the rows of the left table kept', () => {
    expect(startOf('LazyFrame.join').arguments).toEqual([
      { parameter: 'other', setting: { kind: 'word', value: 'step_names' } },
      { parameter: 'left_on', setting: { kind: 'list', items: [{ kind: 'unset' }] } },
      { parameter: 'right_on', setting: { kind: 'list', items: [{ kind: 'unset' }] } },
      { parameter: 'how', setting: { kind: 'word', value: 'left' } },
    ]);
  });

  it('gives a filter a comparison for equality of a field and a value still to be given', () => {
    expect(startOf('LazyFrame.filter').arguments).toEqual([
      {
        parameter: 'predicates',
        setting: {
          kind: 'list',
          items: [
            {
              kind: 'comparison',
              entry: 'Expr.eq',
              field: { kind: 'unset' },
              other: { kind: 'unset' },
            },
          ],
        },
      },
    ]);
  });

  it('gives a pivot the first connected table and the first value of each cell', () => {
    expect(startOf('LazyFrame.pivot').arguments).toContainEqual({
      parameter: 'aggregate_function',
      setting: { kind: 'element', method: 'first' },
    });
    expect(startOf('LazyFrame.pivot').arguments).toContainEqual({
      parameter: 'on_columns',
      setting: { kind: 'word', value: 'step_names' },
    });
  });

  it('gives a new column the name new_column, with what it holds still to be given', () => {
    expect(startOf('LazyFrame.with_columns').arguments).toEqual([
      {
        parameter: 'named_exprs',
        setting: {
          kind: 'pairs',
          entries: [
            {
              key: { kind: 'text', value: 'new_column', shown: 'new_column' },
              value: { kind: 'unset' },
            },
          ],
        },
      },
    ]);
  });

  it('is refused with the place of what is wrong', () => {
    expect(refusal(decodeStepStarts, replaced(stepStarts, ['starts'], {})).message).toBe(
      '$.starts: must be a list, not an object',
    );
    expect(
      refusal(decodeStepStarts, replaced(stepStarts, ['starts', 0, 'arguments', 0, 'setting'], 1))
        .message,
    ).toBe('$.starts[0].arguments[0].setting: must be an object, not a number');
    expect(
      refusal(decodeStepStarts, withField(stepStarts, ['starts', 0], 'extra', 1)).message,
    ).toBe('$.starts[0]: has the field "extra", which the contract lacks');
  });
});
