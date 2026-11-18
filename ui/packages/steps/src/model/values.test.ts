import { describe, expect, it } from 'vitest';

import { valuesLine } from './values';

describe('the values of a column as a line', () => {
  it('are the values the engine wrote, apart by commas', () => {
    expect(valuesLine({ shown: ['2026-03-03', '2026-03-05'], more: 0 })).toBe(
      '2026-03-03, 2026-03-05',
    );
  });

  it('write a null as a dash, since the engine writes no text for it', () => {
    expect(valuesLine({ shown: [null, '2', null], more: 0 })).toBe('—, 2, —');
  });

  it('end in an ellipsis where more values follow those shown', () => {
    expect(valuesLine({ shown: ['1', '2', '3', '4'], more: 3 })).toBe('1, 2, 3, 4, …');
  });

  it('are nothing where there are none', () => {
    expect(valuesLine({ shown: [], more: 0 })).toBe('');
  });
});
