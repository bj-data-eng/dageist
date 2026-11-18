/**
 * @file What a condition starts as before anything is picked in it. The comparison is not the
 * editor's to choose: a new test starts from the first comparison the engine lists, so that it is
 * an entry the catalogue has until the person picks another.
 */

import type { Catalogue } from './catalogue';
import { comparisonOptions } from './catalogue';
import { nextId } from './ids';
import type { Test } from './source';

/**
 * Returns a comparison with nothing picked yet: no field, the first comparison the engine lists,
 * and no value.
 *
 * @throws {Error} When the catalogue lists no comparison, which the engine always does.
 */
export function emptyTest(catalogue: Catalogue): Test {
  const [first] = comparisonOptions(catalogue);
  if (first === undefined) {
    throw new Error('The catalogue lists no comparison to start a test from.');
  }
  return {
    kind: 'test',
    id: nextId('test'),
    column: '',
    comparison: first.entry,
    value: { kind: 'unset' },
  };
}
