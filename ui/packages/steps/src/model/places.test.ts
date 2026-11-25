import { describe, expect, it } from 'vitest';

import type { ContextView, EntryView, ParameterView } from '@dageist/host';
import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { actionsOfSlot, canPick, isDrawn, routeOf, wordsAt } from '../../test-support/places';
import { HOLE_START, bare, calls, chain, col, input } from '../../test-support/expressions';
import { indexCatalogue } from './catalogue';
import { controlOf } from './controls';
import { valueDrawing } from './words';

// The grid test's statement of which words stand at a place and which group of a list each entry
// is picked from. It is test support, written from what the editor draws; no earlier version of it
// existed to fail first.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

/** The entry of the catalogue called `id`. */
function entryOf(id: string): EntryView {
  const found = catalogue.entries.get(id);
  if (found === undefined) throw new Error(`no entry ${id}`);
  return found;
}

/** The place of a context after nothing, after a value, or inside a condition. */
const SELECT: ContextView = { outer: 'select', nested: null };
const START = { context: SELECT, after: { kind: 'start' } } as const;
const VALUE = { context: SELECT, after: { kind: 'value', dtype: 'Int64', shape: 'many' } } as const;
const CONDITION = { context: SELECT, after: { kind: 'condition', state: 'then' } } as const;

/** Whether an argument stands where an expression starts at the context `outer` and `nested`. */
function holds(outer: ContextView['outer'], nested: ContextView['nested']): boolean {
  return wordsAt(catalogue, { outer, nested }, 'start').some((word) => word.word === 'argument');
}

describe('the words that stand at a place', () => {
  it('are a value slot, the start of a chain and an argument where an expression starts', () => {
    expect(wordsAt(catalogue, SELECT, 'start')).toEqual([
      { word: 'value', fromPath: true },
      { word: 'chainStart' },
      { word: 'argument' },
    ]);
  });

  it('are the list that adds a method and the name of a method after a value', () => {
    expect(wordsAt(catalogue, SELECT, 'value')).toEqual([
      { word: 'insertion' },
      { word: 'method', options: false },
    ]);
  });

  it('are none inside a condition, which the editor draws as a `when` with its `then` and `otherwise`', () => {
    expect(wordsAt(catalogue, SELECT, 'condition')).toEqual([]);
  });

  it('are those of a nested position that an option draws, as the value slot of a window’s key', () => {
    const window: ContextView = { outer: 'sort', nested: 'over.partition_by' };
    expect(wordsAt(catalogue, window, 'start')).toContainEqual({ word: 'value', fromPath: true });
  });

  it('are none where no parameter of a step’s head stands at the outer position', () => {
    // A catalogue without the steps that draw the filter position has no word there.
    const without = {
      ...catalogue,
      entries: new Map([...catalogue.entries].filter(([id]) => id !== 'LazyFrame.filter')),
    };
    expect(wordsAt(without, { outer: 'filter', nested: null }, 'start')).toEqual([]);
    expect(wordsAt(without, SELECT, 'start')).not.toEqual([]);
  });

  it('are none where no entry sets a parameter at the nested position', () => {
    // A catalogue without `over` draws no key of a window, so no word stands there.
    const without = {
      ...catalogue,
      entries: new Map([...catalogue.entries].filter(([id]) => id !== 'Expr.over')),
    };
    expect(wordsAt(without, { outer: 'select', nested: 'over.order_by' }, 'start')).toEqual([]);
  });

  it('give the argument only where the position that holds the expression is held by an argument', () => {
    // The first parameter of a step is its argument: select's expressions, a filter's predicates and
    // the keys of a sort. A join's keys come after the table it joins, and a pivot's aggregate after
    // its columns, so they are options.
    expect([holds('select', null), holds('filter', null), holds('sort', null)]).toEqual([
      true,
      true,
      true,
    ]);
    expect([
      holds('join_left', null),
      holds('join_right', null),
      holds('pivot_aggregate', null),
    ]).toEqual([false, false, false]);
    // Inside an expression the keys of a window and the value of a case of a condition are
    // arguments; how a window is ordered is an option, and a condition's predicates are its own.
    expect(holds('select', 'over.partition_by')).toBe(true);
    expect(holds('select', 'condition.value')).toBe(true);
    expect(holds('select', 'over.order_by')).toBe(false);
    expect(holds('select', 'condition.predicate')).toBe(false);
  });

  it('leave out the argument where no entry holds the position with its first parameter', () => {
    // A step whose first parameter is not the expressions it takes, as select's would be after a
    // flag, holds its position with an option, and so does the one that shares the position.
    const flag = entryOf('LazyFrame.sort').parameters.find(
      (parameter) => parameter.kind.kind === 'flag',
    );
    if (flag === undefined) throw new Error('sort has no flag');
    const reorder = (id: string): [string, EntryView] => {
      const entry = entryOf(id);
      return [id, { ...entry, parameters: [flag, ...entry.parameters] }];
    };
    const reordered = {
      ...catalogue,
      entries: new Map([
        ...catalogue.entries,
        reorder('LazyFrame.select'),
        reorder('LazyFrame.with_columns'),
      ]),
    };
    expect(wordsAt(reordered, SELECT, 'start').map((word) => word.word)).toEqual([
      'value',
      'chainStart',
    ]);
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
    expect(routeOf(entryOf('When.then'))).toEqual({ route: 'with', via: 'polars.when' });
    expect(routeOf(entryOf('Then.otherwise'))).toEqual({ route: 'with', via: 'polars.when' });
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
    expect(canPick(catalogue, entryOf('Expr.abs'), VALUE)).toBe(true);
    expect(canPick(catalogue, entryOf('Expr.abs'), START)).toBe(false);
  });

  it('is so for a field, a typed value and `when` at the start of an expression', () => {
    for (const id of ['polars.col', 'polars.lit', 'polars.when']) {
      expect(canPick(catalogue, entryOf(id), START), id).toBe(true);
    }
    expect(canPick(catalogue, entryOf('polars.col'), VALUE)).toBe(false);
  });

  it('is so for `then` after `when`, which comes with it, and for nothing else inside a condition', () => {
    expect(canPick(catalogue, entryOf('When.then'), CONDITION)).toBe(true);
    expect(canPick(catalogue, entryOf('When.then'), START)).toBe(false);
    expect(canPick(catalogue, entryOf('Then.when'), CONDITION)).toBe(false);
    expect(canPick(catalogue, entryOf('polars.int_range'), START)).toBe(false);
  });

  it('is not so in a context where no word stands, whatever the entry', () => {
    const without = {
      ...catalogue,
      entries: new Map([...catalogue.entries].filter(([id]) => id !== 'LazyFrame.filter')),
    };
    const filter = { context: { outer: 'filter', nested: null }, after: VALUE.after } as const;
    expect(canPick(catalogue, entryOf('Expr.abs'), filter)).toBe(true);
    expect(canPick(without, entryOf('Expr.abs'), filter)).toBe(false);
  });

  it('is not so for a link made with `when` where `when` cannot be picked in the same context', () => {
    // Without the steps that draw the filter position there is no word, so `when` cannot start there.
    const without = {
      ...catalogue,
      entries: new Map([...catalogue.entries].filter(([id]) => id !== 'LazyFrame.filter')),
    };
    const place = {
      context: { outer: 'filter', nested: null },
      after: { kind: 'condition', state: 'then' },
    } as const;
    expect(canPick(catalogue, entryOf('When.then'), place)).toBe(true);
    expect(canPick(without, entryOf('When.then'), place)).toBe(false);
  });
});

describe('what can be done to the piece a slot draws', () => {
  it('is adding and taking away for a chain, which a field is drawn as', () => {
    expect(actionsOfSlot(valueDrawing(chain(calls(col('dcn'))), true))).toEqual({
      add: true,
      remove: true,
    });
  });

  it('is taking away only, where the word is one of several, for a connected input and for nothing picked', () => {
    expect(actionsOfSlot(valueDrawing(chain(input('as_of')), true), true)).toEqual({
      add: false,
      remove: true,
    });
    expect(actionsOfSlot(valueDrawing(chain(HOLE_START), true), false)).toEqual({
      add: false,
      remove: false,
    });
    expect(actionsOfSlot(valueDrawing(chain(calls(bare('polars.int_range'))), true))).toEqual({
      add: true,
      remove: true,
    });
  });
});

/** The parameter `name` of the entry `id`. */
function parameterOf(id: string, name: string): ParameterView {
  const found = entryOf(id).parameters.find((parameter) => parameter.name === name);
  if (found === undefined) throw new Error(`no parameter ${name} of ${id}`);
  return found;
}

describe('whether a parameter has a drawing', () => {
  it('is not so where a pair holds pairs, by the control of either side', () => {
    const inner = { kind: 'pairs', key: { kind: 'column' }, value: { kind: 'flag' } } as const;
    const nested: ParameterView = {
      ...parameterOf('Expr.filter', 'constraints'),
      kind: { kind: 'pairs', key: { kind: 'column' }, value: inner },
    };

    expect(isDrawn(controlOf(nested))).toBe(false);
    expect(isDrawn(controlOf(parameterOf('Expr.filter', 'constraints')))).toBe(true);
  });

  it('is so unless one of its arguments is of a kind with none', () => {
    const none = { control: 'none', why: 'no' } as const;
    expect(isDrawn({ shape: 'columns' })).toBe(true);
    expect(isDrawn({ shape: 'one', one: { control: 'flag' } })).toBe(true);
    expect(isDrawn({ shape: 'one', one: none })).toBe(false);
    expect(isDrawn({ shape: 'many', each: none })).toBe(false);
    expect(isDrawn({ shape: 'pairs', key: { control: 'column' }, value: none })).toBe(false);
    expect(isDrawn({ shape: 'perItem', each: 'choose' })).toBe(true);
    expect(isDrawn({ shape: 'perItem', each: 'none', why: 'no' })).toBe(false);
  });
});
