import { describe, expect, it } from 'vitest';

import { isSetting, isValue } from './shape';

describe('the number a window sends another', () => {
  it('may be marked as one the engine read with a fraction, and by nothing else', () => {
    expect(isValue({ kind: 'literal', value: 1 })).toBe(true);
    expect(isValue({ kind: 'literal', value: 1, float: true })).toBe(true);
    expect(isValue({ kind: 'literal', value: 1, float: 'yes' })).toBe(false);
    expect(isValue({ kind: 'literal', value: 1, float: false })).toBe(false);
    expect(isSetting({ kind: 'number', value: 1 })).toBe(true);
    expect(isSetting({ kind: 'number', value: 1, float: true })).toBe(true);
    expect(isSetting({ kind: 'number', value: 1, float: 1 })).toBe(false);
  });

  it('is finite', () => {
    expect(isSetting({ kind: 'number', value: Number.POSITIVE_INFINITY, float: true })).toBe(false);
  });
});
