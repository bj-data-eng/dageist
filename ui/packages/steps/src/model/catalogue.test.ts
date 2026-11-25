import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import {
  HEAD,
  VERBS,
  comparisonOption,
  comparisonOptions,
  indexCatalogue,
  stepOptionParameters,
  verbOf,
} from './catalogue';

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

  it('are the entries the catalogue lists as comparisons', () => {
    expect([...catalogue.comparisons]).toEqual([
      'Expr.eq',
      'Expr.ne',
      'Expr.gt',
      'Expr.ge',
      'Expr.lt',
      'Expr.le',
    ]);
  });

  it('are shown by the sign and meaning of the entry that compares, and any other entry by its name', () => {
    expect(comparisonOption(catalogue, 'Expr.ge')).toEqual({
      entry: 'Expr.ge',
      sign: '≥',
      hint: 'at least',
    });
    expect(comparisonOption(catalogue, 'Expr.shift')).toMatchObject({
      entry: 'Expr.shift',
      sign: 'Expr.shift',
    });
  });
});

/** The names of the parameters of `entry` that a step of it offers as options. */
const names = (entry: string): string[] =>
  stepOptionParameters(catalogue, entry).map((parameter) => parameter.name);

describe('the verbs of the steps the editor offers', () => {
  it('are the seven, each with the parameters its own structure draws left out of its options', () => {
    expect(VERBS).toHaveLength(7);
    expect(verbOf('LazyFrame.sort')).toBe('sort');
    expect(verbOf('LazyFrame.head')).toBeNull();
    // A sort's keys are drawn by the step, so they are not an option; how each is ordered is.
    expect(names('LazyFrame.sort')).toContain('descending');
    expect(names('LazyFrame.sort')).not.toContain('by');
    expect(names('LazyFrame.sort')).not.toContain('more_by');
    // A step the editor draws by its parameters alone offers all it has that the engine offers.
    expect(names('LazyFrame.head')).toContain('n');
  });
});

describe('the parameters a step draws in its head', () => {
  it('are parameters of the step’s own entry', () => {
    for (const verb of VERBS) {
      const entry = catalogue.entries.get(`LazyFrame.${verb}`);
      expect(entry, verb).toBeDefined();
      const parameters = entry?.parameters.map((parameter) => parameter.name) ?? [];
      for (const name of Object.values<string>(HEAD[verb])) {
        expect(parameters, `${verb} ${name}`).toContain(name);
      }
    }
  });

  it('are never offered as options, and the parameters a step does not offer are not either', () => {
    for (const verb of VERBS) {
      for (const name of Object.values<string>(HEAD[verb])) {
        expect(names(`LazyFrame.${verb}`), `${verb} ${name}`).not.toContain(name);
      }
    }
    // A sort's second list of keys and a join's shared key are not offered, though no head draws them.
    expect(names('LazyFrame.sort')).not.toContain('more_by');
    expect(names('LazyFrame.join')).not.toContain('on');
    expect(names('LazyFrame.join')).toContain('how');
  });
});
