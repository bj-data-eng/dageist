import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { HOLE_START, bare, calls, chain, col, input, lit } from '../../test-support/expressions';
import { indexCatalogue } from './catalogue';
import type { Group, ListedWord, PieceKind } from './words';
import {
  GROUPS_OF_WORD,
  actionsOf,
  chainBodyOf,
  lineDrawing,
  listsChainRow,
  listsOf,
  noAddition,
  startWordOf,
  valueDrawing,
} from './words';

/** `true` for a kind of piece the model says nothing can be added after, so that `noAddition` takes its actions. */
type TakesNoAddition<Kind extends PieceKind> =
  ReturnType<typeof actionsOf<Kind>> extends Parameters<typeof noAddition>[0] ? true : false;

// What the editor does with each piece, written from what the editor draws: no earlier version of
// these functions existed to fail first, because the components decided all of this inline.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

/** Every word of the editor that opens a list, in each form it takes. */
const WORDS: readonly ListedWord[] = [
  { word: 'chainStart' },
  { word: 'typedStart' },
  { word: 'method', options: true },
  { word: 'method', options: false },
  { word: 'insertion' },
  { word: 'value', fromPath: true },
  { word: 'value', fromPath: false },
  { word: 'argument' },
  { word: 'step', options: true },
  { word: 'step', options: false },
  { word: 'newStep' },
];

describe('the groups each kind of word may list', () => {
  it('are those of the table the components’ lists are made from, less what a form of the word lacks', () => {
    for (const word of WORDS) {
      const table = GROUPS_OF_WORD[word.word];
      const lacks: readonly Group[] =
        (word.word === 'method' || word.word === 'step') && !word.options
          ? ['options']
          : word.word === 'value' && !word.fromPath
            ? ['functions']
            : [];
      expect(listsOf(word), JSON.stringify(word)).toEqual(
        table.filter((group: Group) => !lacks.includes(group)),
      );
    }
  });

  it('are the whole table for the form of each word that has every group', () => {
    for (const word of WORDS) {
      const full = !('options' in word && !word.options) && !('fromPath' in word && !word.fromPath);
      if (full) expect(listsOf(word), JSON.stringify(word)).toEqual(GROUPS_OF_WORD[word.word]);
    }
  });
});

describe('the groups a word lists', () => {
  it('are the functions, inputs and fields for the word a chain starts from', () => {
    expect(listsOf({ word: 'chainStart' })).toEqual(['functions', 'inputs', 'fields']);
  });

  it('are the typed values first, then the functions, inputs and fields, for the word a typed value starts from', () => {
    // The typed value is retyped where it stands, so its list offers what a value is typed as.
    expect(listsOf({ word: 'typedStart' })).toEqual(['values', 'functions', 'inputs', 'fields']);
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

describe('the groups a kind of word may list', () => {
  it('hold every group the list of a word of that kind lists', () => {
    const words = [
      { word: 'chainStart' },
      { word: 'method', options: true },
      { word: 'method', options: false },
      { word: 'insertion' },
      { word: 'value', fromPath: true },
      { word: 'value', fromPath: false },
      { word: 'argument' },
      { word: 'step', options: true },
      { word: 'step', options: false },
      { word: 'newStep' },
    ] as const;
    for (const word of words) {
      const may: readonly string[] = GROUPS_OF_WORD[word.word];
      for (const group of listsOf(word)) {
        expect(may, `${word.word} ${group}`).toContain(group);
      }
    }
  });
});

describe('the adding of a piece that has none', () => {
  it('is none for a word and an entry of pairs, which nothing can be added after', () => {
    expect(noAddition(actionsOf('word'))).toBeNull();
    expect(noAddition(actionsOf('entry'))).toBeNull();
  });

  it('is accepted by the type of its actions for those kinds and refused for a piece that has some', () => {
    // The assignments below do not compile where the model says otherwise.
    const word: TakesNoAddition<'word'> = true;
    const entry: TakesNoAddition<'entry'> = true;
    const method: TakesNoAddition<'method'> = false;
    const chainStart: TakesNoAddition<'chainStart'> = false;
    const startWord: TakesNoAddition<'startWord'> = false;
    const step: TakesNoAddition<'step'> = false;
    expect([word, entry, method, chainStart, startWord, step]).toEqual([
      true,
      true,
      false,
      false,
      false,
      false,
    ]);
  });
});

describe('how a slot draws the value it holds', () => {
  it('is as a word for nothing, and for a connected input or a number or text typed in that is not in the document', () => {
    expect(valueDrawing(null, true)).toEqual({ drawn: 'word', shape: { kind: 'unset' } });
    expect(valueDrawing(chain(HOLE_START), true)).toEqual({
      drawn: 'word',
      shape: { kind: 'unset' },
    });
    expect(valueDrawing(chain(input('as_of')), false)).toEqual({
      drawn: 'word',
      shape: { kind: 'input', name: 'as_of' },
    });
    expect(valueDrawing(chain(calls(lit({ kind: 'integer', shown: '5' }))), false)).toEqual({
      drawn: 'word',
      shape: { kind: 'literal', shown: '5', text: false },
    });
  });

  it('is as a word that a method can follow for a connected input, and a number or text typed in, in the document', () => {
    expect(valueDrawing(chain(input('as_of')), true)).toEqual({
      drawn: 'start',
      shape: { kind: 'input', name: 'as_of' },
    });
    expect(valueDrawing(chain(calls(lit({ kind: 'integer', shown: '5' }))), true)).toEqual({
      drawn: 'start',
      shape: { kind: 'literal', shown: '5', text: false },
    });
    expect(valueDrawing(chain(calls(lit({ kind: 'string', value: 'a' }))), true)).toEqual({
      drawn: 'start',
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

  it('is as the number or text it starts from for a typed value, not as the function that holds it', () => {
    expect(chainBodyOf(chain(calls(lit({ kind: 'integer', shown: '5' }))))).toEqual({
      body: 'typed',
      shown: '5',
      text: false,
    });
    expect(
      chainBodyOf(chain(calls(lit({ kind: 'string', value: 'a' })), [bare('Expr.abs')])),
    ).toEqual({ body: 'typed', shown: 'a', text: true });
  });
});

describe('the word a chain starts from', () => {
  it('lists the typed values where the chain starts from a number or text typed in, so it can be retyped', () => {
    const typed = chainBodyOf(
      chain(calls(lit({ kind: 'integer', shown: '5' })), [bare('Expr.abs')]),
    );

    expect(startWordOf(typed)).toEqual({ word: 'typedStart' });
    expect(listsOf(startWordOf(typed))).toContain('values');
  });

  it('is the word that lists no typed value for a field, an input and nothing yet', () => {
    for (const body of [
      chainBodyOf(chain(calls(col('dcn')))),
      chainBodyOf(chain(input('as_of'))),
      chainBodyOf(chain(HOLE_START)),
    ]) {
      expect(startWordOf(body)).toEqual({ word: 'chainStart' });
      expect(listsOf(startWordOf(body))).not.toContain('values');
    }
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

  it('is adding a method after it, and taking it away where the word is one of several, for a connected input or a typed value in the document', () => {
    expect(actionsOf('startWord', true)).toEqual({ add: true, remove: true });
    expect(actionsOf('startWord', false)).toEqual({ add: true, remove: false });
  });

  it('is taking it away only for an entry of pairs', () => {
    expect(actionsOf('entry')).toEqual({ add: false, remove: true });
  });
});
