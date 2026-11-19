import { describe, expect, it } from 'vitest';

import { allowedAt } from '../test-support/grid';
import type { Grid } from '../test-support/grid';

/** A grid of 40 entries, `e0` to `e39`, at one place, which holds `words`. */
function gridOf(words: readonly number[]): Grid {
  return {
    entries: Array.from({ length: 40 }, (_, at) => `e${at}`),
    contexts: [],
    afters: [],
    allowed: [[words]],
  };
}

describe('the entries a grid allows at a place', () => {
  it('are those whose bit is set in the words, the bit entry % 32 of the word entry / 32', () => {
    // Bit 0 and bit 31 of the first word, and bits 0 and 7 of the second: e0, e31, e32 and e39.
    expect(allowedAt(gridOf([2 ** 31 + 1, 129]), 0, 0)).toEqual(['e0', 'e31', 'e32', 'e39']);
    expect(allowedAt(gridOf([0, 0]), 0, 0)).toEqual([]);
  });

  it('are refused when the words are fewer or more than the entries need', () => {
    expect(() => allowedAt(gridOf([1]), 0, 0)).toThrow('1 words, not 2');
    expect(() => allowedAt(gridOf([1, 1, 1]), 0, 0)).toThrow('3 words, not 2');
  });

  it('are refused when a word is not a whole number of 32 bits', () => {
    for (const word of [2 ** 32, -1, 1.5, Number.NaN]) {
      expect(() => allowedAt(gridOf([0, word]), 0, 0), String(word)).toThrow('not of 32 bits');
    }
    expect(allowedAt(gridOf([2 ** 32 - 1, 0]), 0, 0)).toHaveLength(32);
  });

  it('are refused for a place the grid does not hold', () => {
    expect(() => allowedAt(gridOf([0, 0]), 1, 0)).toThrow('no place');
    expect(() => allowedAt(gridOf([0, 0]), 0, 1)).toThrow('no place');
  });
});
