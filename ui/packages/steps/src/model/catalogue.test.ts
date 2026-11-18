import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { comparisonOptions, indexCatalogue, signOf } from './catalogue';

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

  it('are found by the entry that compares, which is how a test holds one', () => {
    expect(signOf(catalogue, 'Expr.ge')).toBe('≥');
    expect(signOf(catalogue, 'Expr.ne')).toBe('≠');
    // An entry that does not compare, and one the catalogue lacks, have no sign.
    expect(signOf(catalogue, 'Expr.shift')).toBeNull();
    expect(signOf(catalogue, 'Expr.nothing')).toBeNull();
  });
});
