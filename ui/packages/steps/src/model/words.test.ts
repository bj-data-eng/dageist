import { describe, expect, it } from 'vitest';

import type { EntryView } from '@dageist/host';
import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { HOLE_START, bare, calls, chain, col, input, lit } from '../../test-support/expressions';
import { indexCatalogue } from './catalogue';
import {
  actionsOf,
  canPick,
  chainBodyOf,
  lineDrawing,
  listsChainRow,
  listsOf,
  routeOf,
  valueDrawing,
  wordsAt,
} from './words';

// What the editor does with each piece, written from what the editor draws: no earlier version of
// these functions existed to fail first, because the components decided all of this inline.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

/** The entry of the catalogue called `id`. */
function entryOf(id: string): EntryView {
  const found = catalogue.entries.get(id);
  if (found === undefined) throw new Error(`no entry ${id}`);
  return found;
}

describe('the groups a word lists', () => {
  it('are the functions, inputs and fields for the word a chain starts from', () => {
    expect(listsOf({ word: 'chainStart' })).toEqual(['functions', 'inputs', 'fields']);
  });

  it('are the options, where the method has some, then the methods, for the name of a method', () => {
    expect(listsOf({ word: 'method', options: true })).toEqual(['options', 'methods']);
    expect(listsOf({ word: 'method', options: false })).toEqual(['methods']);
  });

  it('are the methods alone for the list that adds one after a piece', () => {
    expect(listsOf({ word: 'insertion' })).toEqual(['methods']);
  });

  it('are the chain row, typed values, inputs and fields for a value, and functions where it is in the document', () => {
    expect(listsOf({ word: 'value', fromPath: false })).toEqual([
      'chain',
      'values',
      'inputs',
      'fields',
    ]);
    expect(listsOf({ word: 'value', fromPath: true })).toEqual([
      'chain',
      'values',
      'inputs',
      'fields',
      'functions',
    ]);
  });

  it('are the typed values, inputs and fields for the argument of a method', () => {
    expect(listsOf({ word: 'argument' })).toEqual(['values', 'inputs', 'fields']);
  });

  it('are the options, where there are some, then the steps for the name of a step, and the steps for a new one', () => {
    expect(listsOf({ word: 'step', options: true })).toEqual(['options', 'steps']);
    expect(listsOf({ word: 'step', options: false })).toEqual(['steps']);
    expect(listsOf({ word: 'newStep' })).toEqual(['steps']);
  });

  it('hold the row that starts a chain only while nothing is typed', () => {
    expect(listsChainRow('')).toBe(true);
    expect(listsChainRow('a')).toBe(false);
  });
});

describe('the words that stand at a place', () => {
  it('are a value slot, the start of a chain and an argument where an expression starts', () => {
    expect(wordsAt('start')).toEqual([
      { word: 'value', fromPath: true },
      { word: 'chainStart' },
      { word: 'argument' },
    ]);
  });

  it('are the list that adds a method and the name of a method after a value', () => {
    expect(wordsAt('value')).toEqual([{ word: 'insertion' }, { word: 'method', options: false }]);
  });

  it('are none inside a condition, which the editor draws as a `when` with its `then` and `otherwise`', () => {
    expect(wordsAt('condition')).toEqual([]);
  });
});

describe('how an entry is picked', () => {
  it('is from the methods for a method of an expression', () => {
    expect(routeOf(entryOf('Expr.abs'))).toEqual({ route: 'group', group: 'methods' });
  });

  it('is from the fields for `col` and from the typed values for `lit`', () => {
    expect(routeOf(entryOf('polars.col'))).toEqual({ route: 'group', group: 'fields' });
    expect(routeOf(entryOf('polars.lit'))).toEqual({ route: 'group', group: 'values' });
  });

  it('is from the functions for `when` and `element`, the functions a chain starts from', () => {
    expect(routeOf(entryOf('polars.when'))).toEqual({ route: 'group', group: 'functions' });
    expect(routeOf(entryOf('polars.element'))).toEqual({ route: 'group', group: 'functions' });
  });

  it('is with `when` for the `then` and the `otherwise` of the one case', () => {
    expect(routeOf(entryOf('When.then'))).toEqual({ route: 'with', group: 'functions' });
    expect(routeOf(entryOf('Then.otherwise'))).toEqual({ route: 'with', group: 'functions' });
  });

  it('is from the steps for a step the editor offers, and none for a step it does not', () => {
    expect(routeOf(entryOf('LazyFrame.sort'))).toEqual({ route: 'group', group: 'steps' });
    expect(routeOf(entryOf('LazyFrame.head'))).toEqual({ route: 'none' });
  });

  it('is none for a function a chain does not start from, and for a further case of a condition', () => {
    expect(routeOf(entryOf('polars.int_range'))).toEqual({ route: 'none' });
    expect(routeOf(entryOf('Then.when'))).toEqual({ route: 'none' });
    expect(routeOf(entryOf('ChainedThen.otherwise'))).toEqual({ route: 'none' });
  });
});

describe('whether an entry can be picked at a place', () => {
  it('is so for a method after a value, and not at the start of an expression', () => {
    expect(canPick(entryOf('Expr.abs'), 'value')).toBe(true);
    expect(canPick(entryOf('Expr.abs'), 'start')).toBe(false);
  });

  it('is so for a field, a typed value and `when` at the start of an expression', () => {
    for (const id of ['polars.col', 'polars.lit', 'polars.when']) {
      expect(canPick(entryOf(id), 'start'), id).toBe(true);
    }
    expect(canPick(entryOf('polars.col'), 'value')).toBe(false);
  });

  it('is so for `then` after `when`, which comes with it, and for nothing else inside a condition', () => {
    expect(canPick(entryOf('When.then'), 'condition')).toBe(true);
    expect(canPick(entryOf('When.then'), 'start')).toBe(false);
    expect(canPick(entryOf('Then.when'), 'condition')).toBe(false);
    expect(canPick(entryOf('polars.int_range'), 'start')).toBe(false);
  });
});

describe('how a slot draws the value it holds', () => {
  it('is as a word for nothing, a connected input, and a number or text typed in', () => {
    expect(valueDrawing(null, true)).toEqual({ drawn: 'word', shape: { kind: 'unset' } });
    expect(valueDrawing(chain(HOLE_START), true)).toEqual({
      drawn: 'word',
      shape: { kind: 'unset' },
    });
    expect(valueDrawing(chain(input('as_of')), true)).toEqual({
      drawn: 'word',
      shape: { kind: 'input', name: 'as_of' },
    });
    expect(valueDrawing(chain(calls(lit({ kind: 'integer', shown: '5' }))), true)).toEqual({
      drawn: 'word',
      shape: { kind: 'literal', shown: '5', text: false },
    });
    expect(valueDrawing(chain(calls(lit({ kind: 'string', value: 'a' }))), true)).toEqual({
      drawn: 'word',
      shape: { kind: 'literal', shown: 'a', text: true },
    });
  });

  it('is as a chain for a field and for anything with methods, once it is in the document', () => {
    expect(valueDrawing(chain(calls(col('dcn'))), true)).toEqual({ drawn: 'chain' });
    expect(valueDrawing(chain(calls(col('dcn')), [bare('Expr.abs')]), true)).toEqual({
      drawn: 'chain',
    });
    expect(valueDrawing(chain(input('as_of'), [bare('Expr.abs')]), true)).toEqual({
      drawn: 'chain',
    });
  });

  it('is as a word for a field not in the document yet, and as nothing for a chain not in it', () => {
    expect(valueDrawing(chain(calls(col('dcn'))), false)).toEqual({
      drawn: 'word',
      shape: { kind: 'column', name: 'dcn' },
    });
    expect(valueDrawing(chain(calls(col('dcn')), [bare('Expr.abs')]), false)).toEqual({
      drawn: 'word',
      shape: { kind: 'unset' },
    });
  });
});

describe('how a chain is laid out', () => {
  const when = chain(calls(bare('polars.when')), [bare('When.then'), bare('Then.otherwise')]);

  it('is as lines for a `when` with its `then` and `otherwise`', () => {
    expect(chainBodyOf(when)).toMatchObject({ body: 'when', layout: { before: 2 } });
  });

  it('is as one word for `element`', () => {
    expect(chainBodyOf(chain(calls(bare('polars.element'))))).toEqual({ body: 'element' });
  });

  it('is as a name with what it was given for a function with no word of its own, and for a `when` with no case', () => {
    expect(chainBodyOf(chain(calls(bare('polars.int_range'))))).toEqual({ body: 'function' });
    expect(chainBodyOf(chain(calls(bare('polars.when'))))).toEqual({ body: 'function' });
  });

  it('is as the word it starts from for a field, an input and nothing yet', () => {
    expect(chainBodyOf(chain(calls(col('dcn'))))).toEqual({ body: 'start' });
    expect(chainBodyOf(chain(input('as_of')))).toEqual({ body: 'start' });
    expect(chainBodyOf(chain(HOLE_START))).toEqual({ body: 'start' });
  });
});

describe('how a line of a condition is drawn', () => {
  it('is as a comparison for a field and one comparison of the entries that compare', () => {
    const line = chain(calls(col('dcn')), [bare('Expr.eq')]);
    expect(lineDrawing(line, catalogue.comparisons)).toMatchObject({
      drawn: 'condition',
      test: { column: 'dcn' },
    });
  });

  it('is as a chain for anything else', () => {
    expect(lineDrawing(chain(calls(col('dcn'))), catalogue.comparisons)).toEqual({
      drawn: 'chain',
    });
    expect(
      lineDrawing(chain(calls(col('dcn')), [bare('Expr.abs')]), catalogue.comparisons),
    ).toEqual({ drawn: 'chain' });
  });
});

describe('what can be done to a piece', () => {
  it('is adding after it and taking it away for the start of a chain, a method, a step and a line', () => {
    for (const kind of [
      'chainStart',
      'method',
      'step',
      'conditionHead',
      'conditionLine',
    ] as const) {
      expect(actionsOf(kind), kind).toEqual({ add: true, remove: true });
    }
  });

  it('is taking it away only, where the word is one of several, for a value drawn as one word', () => {
    expect(actionsOf('word', true)).toEqual({ add: false, remove: true });
    expect(actionsOf('word', false)).toEqual({ add: false, remove: false });
  });

  it('is taking it away only for an entry of pairs', () => {
    expect(actionsOf('entry')).toEqual({ add: false, remove: true });
  });
});
