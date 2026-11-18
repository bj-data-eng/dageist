import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import {
  comparisonChoices,
  comparisonOption,
  comparisonOptions,
  indexCatalogue,
} from './catalogue';
import { IS_IN, asComparison } from './source';

// The catalogue as the host wrote it, with the signs of the language file.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

describe('the comparisons of a test', () => {
  it('are the engine’s entries that compare, each with its sign and what it means, in the order they are offered', () => {
    expect(comparisonOptions(catalogue)).toEqual([
      { entry: 'Expr.eq', sign: '==', hint: 'equal to' },
      { entry: 'Expr.ne', sign: '≠', hint: 'not equal to' },
      { entry: 'Expr.gt', sign: '>', hint: 'greater than' },
      { entry: 'Expr.ge', sign: '≥', hint: 'at least' },
      { entry: 'Expr.lt', sign: '<', hint: 'less than' },
      { entry: 'Expr.le', sign: '≤', hint: 'at most' },
    ]);
  });

  it('are the entries the catalogue lists, which a test may hold besides membership in a list', () => {
    expect([...catalogue.comparisons]).toEqual([
      'Expr.eq',
      'Expr.ne',
      'Expr.gt',
      'Expr.ge',
      'Expr.lt',
      'Expr.le',
    ]);
    expect(asComparison('Expr.ge', catalogue.comparisons)).toBe('Expr.ge');
    expect(asComparison(IS_IN, catalogue.comparisons)).toBe(IS_IN);
    // `>=` is how Python spells it, and `Expr.shift` is an entry that does not compare.
    expect(asComparison('>=', catalogue.comparisons)).toBeNull();
    expect(asComparison('Expr.shift', catalogue.comparisons)).toBeNull();
    expect(asComparison('', catalogue.comparisons)).toBeNull();
  });

  it('are shown by the sign and meaning of the entry that compares, and membership by its own name', () => {
    const ge = asComparison('Expr.ge', catalogue.comparisons);
    if (ge === null) throw new Error('Expr.ge compares');
    expect(comparisonOption(catalogue, ge)).toEqual({ entry: ge, sign: '≥', hint: 'at least' });
    expect(comparisonChoices(catalogue).map((choice) => choice.sign)).toEqual([
      '==',
      '≠',
      '>',
      '≥',
      '<',
      '≤',
      'is_in',
    ]);
    expect(comparisonOption(catalogue, IS_IN)).toEqual({
      entry: IS_IN,
      sign: 'is_in',
      hint: 'one of a list',
    });
  });
});
