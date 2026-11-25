import { describe, expect, it, vi } from 'vitest';

import { catalogue, compare, findGaps, grid, STARTS } from '../../test-support/grid-gaps';
import type * as CatalogueModule from './catalogue';
import { GAPS } from './limits';

// The editor starts a chain from one more function than it does: `polars.date`, which the grid
// allows and the declared gaps still say the editor cannot offer. A declared gap for an entry
// that has become one the editor offers is stale, and the grid test must say so.
vi.mock('./catalogue', async (importOriginal) => {
  const original = await importOriginal<typeof CatalogueModule>();
  return {
    ...original,
    START_FUNCTIONS: new Set([...original.START_FUNCTIONS, 'polars.date']),
  };
});

describe('a declared entry gap for an entry the editor now offers', () => {
  it('is found stale, though the list still says the editor cannot offer the entry', () => {
    const found = findGaps(grid, catalogue, STARTS, grid.contexts);

    expect(found.gaps.has('entry polars.date')).toBe(false);
    expect(compare(found, GAPS)).toEqual({ undeclared: [], stale: ['entry polars.date'] });
  });
});
