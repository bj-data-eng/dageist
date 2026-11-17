import { describe, expect, it } from 'vitest';

import { DecodeError } from './decode';
import { decodeFits, decodeOffers, decodeOptions } from './decode-offers';
import fitsArgumentFixture from '../fixtures/fits-argument.json';
import fitsStartFixture from '../fixtures/fits-start.json';
import offersConditionFixture from '../fixtures/offers-condition.json';
import offersStartFixture from '../fixtures/offers-start.json';
import offersValueFixture from '../fixtures/offers-value.json';
import optionsFixture from '../fixtures/options.json';
import { replaced, withField, without } from '../test-support/tamper';

// The fixtures are written from Rust by the host's snapshot test. They are read as `unknown`,
// as the host's answer is, and the decoder is what says what they are.
const offersStart: unknown = offersStartFixture;
const offersValue: unknown = offersValueFixture;
const offersCondition: unknown = offersConditionFixture;
const fitsStart: unknown = fitsStartFixture;
const fitsArgument: unknown = fitsArgumentFixture;
const options: unknown = optionsFixture;

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
    expect(entries).toHaveLength(35);
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
      literal: { kind: 'integer', value: 1 },
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
    expect(fits).toHaveLength(8);
    // Candidates are asked about in the order they were sent: a Boolean field, a whole number
    // field, a Boolean value, a list of dates, a whole number typed, a number with a fraction,
    // text, and a dtype the tables cover no class of.
    const dtypes = fits.map((fit) => (fit.kind === 'judged' ? fit.dtype : null));
    expect(dtypes).toEqual([
      'Boolean',
      'Int64',
      'Boolean',
      'Date',
      'Int32',
      'Float64',
      'String',
      null,
    ]);
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
    expect(decodeFits(fitsStart).fits[7]).toEqual({
      kind: 'unjudged',
      reason: 'the tables cover no class of the dtype Categorical',
    });
  });

  it('refuse a candidate that does not fit the argument, with the engine reason', () => {
    const kinds = decodeFits(fitsArgument).fits.map((fit) =>
      fit.kind === 'judged' ? fit.verdict.kind : fit.kind,
    );
    // `dt.offset_by` takes a duration as text: a field of text, a connected text and typed text
    // fit; a whole number field and a whole number typed do not.
    expect(kinds).toEqual(['allowed', 'refused', 'allowed', 'allowed', 'refused']);
  });

  it('are refused with the place of what is wrong', () => {
    expect(refusal(decodeFits, replaced(fitsStart, ['fits', 0, 'kind'], 'maybe')).message).toBe(
      '$.fits[0].kind: must be one of "judged", "unjudged", not "maybe"',
    );
    expect(refusal(decodeFits, without(fitsStart, ['fits', 7], 'reason')).message).toBe(
      '$.fits[7]: is missing the field "reason"',
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
