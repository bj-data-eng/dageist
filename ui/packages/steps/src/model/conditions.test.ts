import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import { at } from '../../../host/test-support/tamper';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { indexCatalogue } from './catalogue';
import { emptyTest } from './conditions';
import { translate } from './document';
import { NO_OPTIONS } from './settings';
import type { Condition } from './source';
import type { Step } from './steps';

// The catalogue the host serves, as the editor reads it.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

/** A filter over `condition`. */
function filtering(condition: Condition): Step {
  return { id: 'keep', verb: 'filter', condition, options: NO_OPTIONS };
}

describe('a test with nothing picked yet', () => {
  it('compares as the first comparison the engine lists, with no field and no value', () => {
    expect(emptyTest(catalogue)).toMatchObject({
      kind: 'test',
      column: '',
      comparison: 'Expr.eq',
      value: { kind: 'unset' },
    });
  });

  it('is a test of its own each time, with an identifier that stays with it', () => {
    expect(emptyTest(catalogue).id).not.toBe(emptyTest(catalogue).id);
  });

  it('is written as a call of an entry the catalogue has, so that the engine can build the step', () => {
    const { text, steps } = translate(catalogue, [filtering(emptyTest(catalogue))]);
    expect(steps[0]?.kind).toBe('sent');
    const document: unknown = JSON.parse(text);
    const link = at(document, [
      'steps',
      0,
      'call',
      'arguments',
      'predicates',
      'many',
      0,
      'expression',
      'then',
      0,
    ]);
    const entry = at(link, ['entry']);
    expect(entry).toBe('Expr.eq');
    expect(typeof entry === 'string' && catalogue.entries.has(entry)).toBe(true);
  });

  it('is refused where the catalogue lists no comparison to start from', () => {
    const none = indexCatalogue({ ...catalogue.view, comparisons: [] });
    expect(() => emptyTest(none)).toThrow(/no comparison/);
  });
});
