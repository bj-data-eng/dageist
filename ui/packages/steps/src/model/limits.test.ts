import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { sorted } from '../../test-support/sorted';
import { indexCatalogue } from './catalogue';
import { GAPS, coverKey, editorLimit } from './limits';

// The declared gaps, written from what the editor is known not to draw. That the editor really has
// each of them, and no other, is what the grid test checks against the engine's own grid.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

describe('the declared gaps', () => {
  it('are a further case of a condition, the functions that do not start a chain, what a field, a typed value and a condition leave unset, what cannot follow a word, the steps no list adds and the parameters of a step that are not offered, a report that says nothing, a flag that cannot be typed and a flag and several dtypes that are not shown', () => {
    expect(sorted(GAPS.map((gap) => coverKey(gap.cover)))).toEqual(
      sorted([
        'entry ChainedThen.otherwise',
        'entry ChainedThen.when',
        'entry ChainedWhen.then',
        'entry Then.when',
        'entry polars.date',
        'entry polars.date_range',
        'entry polars.datetime',
        'entry polars.datetime_range',
        'entry polars.duration',
        'entry polars.from_epoch',
        'entry polars.int_range',
        'entry polars.linear_space',
        'entry polars.repeat',
        'entry polars.time',
        'entry polars.time_range',
        'parameter polars.col more_names',
        'parameter polars.lit allow_object',
        'parameter polars.lit dtype',
        'parameter polars.when constraints',
        'report notCountedAfterRowsFailure',
        'shown literal flag',
        'shown resolution several',
        'start connectedList',
        'start connectedValue',
        'start typedFloat',
        'start typedInteger',
        'start typedText',
        'step LazyFrame.drop',
        'step LazyFrame.drop_nans',
        'step LazyFrame.drop_nulls',
        'step LazyFrame.fill_nan',
        'step LazyFrame.first',
        'step LazyFrame.gather_every',
        'step LazyFrame.head',
        'step LazyFrame.last',
        'step LazyFrame.limit',
        'step LazyFrame.reverse',
        'step LazyFrame.slice',
        'step LazyFrame.tail',
        'step LazyFrame.with_row_index',
        'stepParameter LazyFrame.join on',
        'stepParameter LazyFrame.sort more_by',
        'typed flag',
      ]),
    );
  });

  it('are each declared once, and each says why', () => {
    const keys = GAPS.map((gap) => coverKey(gap.cover));
    expect(new Set(keys).size).toBe(keys.length);
    for (const gap of GAPS) {
      expect(gap.reason.length, coverKey(gap.cover)).toBeGreaterThan(20);
    }
  });

  it('name the issue that is to draw what a connected input, a typed value and a report leave undrawn', () => {
    for (const gap of GAPS) {
      if (gap.cover.kind === 'start' || gap.cover.kind === 'report') {
        expect(gap.reason, coverKey(gap.cover)).toContain('#171');
      }
    }
  });

  it('name the issue that is to show a flag a document holds, to type one and to show the dtypes of several columns', () => {
    const keys = ['typed flag', 'shown literal flag', 'shown resolution several'];
    for (const gap of GAPS.filter((held) => keys.includes(coverKey(held.cover)))) {
      expect(gap.reason, coverKey(gap.cover)).toContain('#177');
    }
    expect(GAPS.filter((gap) => keys.includes(coverKey(gap.cover)))).toHaveLength(3);
  });

  it('are keyed by what they cover, so that two places or two parameters are two gaps', () => {
    expect(coverKey({ kind: 'place', outer: 'select', nested: null })).not.toBe(
      coverKey({ kind: 'place', outer: 'select', nested: 'over.order_by' }),
    );
    expect(coverKey({ kind: 'parameter', entry: 'a', parameter: 'b' })).not.toBe(
      coverKey({ kind: 'parameter', entry: 'a', parameter: 'c' }),
    );
    expect(coverKey({ kind: 'parameterKind', of: 'one of pairs' })).toBe(
      'parameterKind one of pairs',
    );
  });
});

/** The entry of the catalogue called `id`. */
function entry(id: string) {
  const found = catalogue.entries.get(id);
  if (found === undefined) throw new Error(`no entry ${id}`);
  return found;
}

describe('what a list says of an entry the editor cannot pick', () => {
  it('is why a further case of a condition is not drawn', () => {
    const reason = 'the editor draws one case of a when; a further case follows otherwise';
    for (const id of [
      'ChainedThen.otherwise',
      'ChainedThen.when',
      'ChainedWhen.then',
      'Then.when',
    ]) {
      expect(editorLimit(entry(id)), id).toBe(reason);
    }
  });

  it('is nothing for an entry the editor draws: a method, a field, a typed value, when, then and otherwise', () => {
    for (const id of [
      'Expr.abs',
      'polars.col',
      'polars.lit',
      'polars.when',
      'polars.element',
      'When.then',
      'Then.otherwise',
    ]) {
      expect(editorLimit(entry(id)), id).toBeNull();
    }
  });
});
