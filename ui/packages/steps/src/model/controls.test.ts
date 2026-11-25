import { describe, expect, it } from 'vitest';

import type { EntryView, ParameterView } from '@dageist/host';
import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import {
  controlOf,
  methodOptionParameters,
  oneControlOf,
  settingOf,
  startsWithEngineWord,
} from './controls';

// What each kind and shape of parameter is drawn as, written from what the catalogue's entries are
// and what the drawings of the editor are. There is no earlier version of these functions to have
// failed first: the components used to decide this inline, and these tests state what they drew.
const catalogue = decodeCatalogue(catalogueFixture);

/** The entry of the catalogue called `id`. */
function entryOf(id: string): EntryView {
  const found = catalogue.entries.find((entry) => entry.id === id);
  if (found === undefined) throw new Error(`no entry ${id}`);
  return found;
}

/** The parameter `name` of the entry `id`. */
function parameterOf(id: string, name: string): ParameterView {
  const found = entryOf(id).parameters.find((parameter) => parameter.name === name);
  if (found === undefined) throw new Error(`no parameter ${name} of ${id}`);
  return found;
}

describe('how an argument of one kind is drawn', () => {
  it('is by its own control for every kind that holds one value', () => {
    expect(oneControlOf({ kind: 'expression' })).toEqual({ control: 'expression' });
    expect(oneControlOf({ kind: 'column' })).toEqual({ control: 'column' });
    expect(oneControlOf({ kind: 'flag' })).toEqual({ control: 'flag' });
    expect(oneControlOf({ kind: 'dtype' })).toEqual({ control: 'dtype' });
    expect(oneControlOf({ kind: 'table' })).toEqual({ control: 'table' });
  });

  it('is a typed value of its own type, or the typed kind it is for a duration, a pattern or a path', () => {
    expect(oneControlOf({ kind: 'value', type: 'integer' })).toEqual({
      control: 'literal',
      type: 'integer',
    });
    expect(oneControlOf({ kind: 'value', type: 'any' })).toEqual({
      control: 'literal',
      type: 'any',
    });
    expect(oneControlOf({ kind: 'duration' })).toEqual({ control: 'literal', type: 'duration' });
    expect(oneControlOf({ kind: 'pattern', plainTextFlag: 'literal' })).toEqual({
      control: 'literal',
      type: 'pattern',
    });
    expect(oneControlOf({ kind: 'path' })).toEqual({ control: 'literal', type: 'path' });
  });

  it('keeps the words of a choice and the word for none of a time zone', () => {
    const words = [{ word: 'earliest', shown: 'earliest' }];
    expect(oneControlOf({ kind: 'choice', words })).toEqual({ control: 'choice', words });
    const none = { shown: 'None', hint: 'no zone' };
    expect(oneControlOf({ kind: 'timeZone', none })).toEqual({ control: 'timeZone', none });
  });

  it('is none for pairs, which an entry of pairs does not hold', () => {
    expect(
      oneControlOf({ kind: 'pairs', key: { kind: 'column' }, value: { kind: 'flag' } }),
    ).toEqual({ control: 'none', why: 'pairs inside pairs are not drawn' });
  });
});

describe('how a parameter is drawn', () => {
  it('is one argument for a parameter of one', () => {
    expect(controlOf(parameterOf('Expr.shift', 'n'))).toEqual({
      shape: 'one',
      one: { control: 'expression' },
    });
    expect(controlOf(parameterOf('Expr.cast', 'strict'))).toEqual({
      shape: 'one',
      one: { control: 'flag' },
    });
  });

  it('is a list of one kind for a parameter that holds several, and folded fields for fields', () => {
    expect(controlOf(parameterOf('Expr.sort_by', 'by'))).toEqual({
      shape: 'many',
      each: { control: 'expression' },
    });
    expect(controlOf(parameterOf('polars.col', 'more_names'))).toEqual({ shape: 'columns' });
  });

  it('is entries of pairs for pairs, a key beside a value, each of its own kind', () => {
    expect(controlOf(parameterOf('Expr.filter', 'constraints'))).toEqual({
      shape: 'pairs',
      key: { control: 'column' },
      value: { control: 'expression' },
    });
  });

  it('is a choice for each item for a flag that holds one value for each item of another', () => {
    expect(controlOf(parameterOf('Expr.sort_by', 'descending'))).toEqual({
      shape: 'perItem',
      each: 'choose',
    });
  });

  it('is not drawn for a value for each item of a kind that a row of a list has no room to type', () => {
    const typed: ParameterView = {
      ...parameterOf('Expr.sort_by', 'descending'),
      shown: 'every',
      kind: { kind: 'value', type: 'integer' },
    };

    expect(controlOf(typed)).toEqual({
      shape: 'perItem',
      each: 'none',
      why: 'every is not drawn here',
    });
  });
});

describe('what an option waits for', () => {
  it('is the engine’s word for a flag and a choice, and nothing for any other kind', () => {
    expect(startsWithEngineWord({ kind: 'flag' })).toBe(true);
    expect(startsWithEngineWord({ kind: 'choice', words: [] })).toBe(true);
    expect(startsWithEngineWord({ kind: 'dtype' })).toBe(false);
    expect(startsWithEngineWord({ kind: 'expression' })).toBe(false);
  });
});

describe('how a parameter of an entry is set', () => {
  it('is the argument of a method for its first parameter when that is an expression', () => {
    const shift = entryOf('Expr.shift');
    expect(settingOf(shift, parameterOf('Expr.shift', 'n'))).toEqual({
      by: 'argument',
      control: { shape: 'one', one: { control: 'expression' } },
    });
    expect(settingOf(shift, parameterOf('Expr.shift', 'fill_value'))).toMatchObject({
      by: 'option',
    });
  });

  it('is an option for every other offered parameter, of the kind it has', () => {
    expect(settingOf(entryOf('Expr.cast'), parameterOf('Expr.cast', 'dtype'))).toEqual({
      by: 'option',
      control: { shape: 'one', one: { control: 'dtype' } },
    });
  });

  it('is nothing for a parameter the engine does not offer, which the signature has and nothing here changes', () => {
    expect(
      settingOf(entryOf('Expr.str.contains'), parameterOf('Expr.str.contains', 'strict')),
    ).toEqual({ by: 'unoffered' });
  });

  it('is the entry’s own word for the one parameter of a field, a typed value or a condition that it sets', () => {
    expect(settingOf(entryOf('polars.col'), parameterOf('polars.col', 'name'))).toEqual({
      by: 'entry',
    });
    expect(settingOf(entryOf('polars.lit'), parameterOf('polars.lit', 'value'))).toEqual({
      by: 'entry',
    });
    expect(settingOf(entryOf('polars.when'), parameterOf('polars.when', 'predicates'))).toEqual({
      by: 'entry',
    });
  });

  it('is unset for the rest of the parameters of those entries', () => {
    expect(settingOf(entryOf('polars.col'), parameterOf('polars.col', 'more_names'))).toEqual({
      by: 'unset',
    });
    expect(settingOf(entryOf('polars.lit'), parameterOf('polars.lit', 'dtype'))).toEqual({
      by: 'unset',
    });
    expect(settingOf(entryOf('polars.lit'), parameterOf('polars.lit', 'allow_object'))).toEqual({
      by: 'unset',
    });
    expect(settingOf(entryOf('polars.when'), parameterOf('polars.when', 'constraints'))).toEqual({
      by: 'unset',
    });
  });
});

describe('the parameters of a method that are added as options', () => {
  it('are those the engine offers, but the argument that is drawn where the method is', () => {
    expect(
      methodOptionParameters(entryOf('Expr.shift')).map((parameter) => parameter.name),
    ).toEqual(['fill_value']);
    expect(
      methodOptionParameters(entryOf('Expr.str.contains')).map((parameter) => parameter.name),
    ).toEqual(['pattern', 'literal']);
    expect(
      methodOptionParameters(entryOf('Expr.sort_by')).map((parameter) => parameter.name),
    ).toEqual(['more_by', 'descending', 'nulls_last', 'multithreaded', 'maintain_order']);
  });

  it('are none for a method that is not in the catalogue', () => {
    expect(methodOptionParameters(null)).toEqual([]);
  });
});
