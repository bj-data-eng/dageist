import { describe, expect, it } from 'vitest';

import { literalSetting, literalValue, literalWords, valueWords } from './literal';

describe('a number or text the engine read', () => {
  it('is held by a chain as the value it read, a number with a fraction marked as one', () => {
    expect(literalValue({ kind: 'integer', value: 12 })).toEqual({ kind: 'literal', value: 12 });
    expect(literalValue({ kind: 'text', value: '12' })).toEqual({ kind: 'literal', value: '12' });
    // A whole number read with a fraction stays a float: it is not an integer for being whole.
    expect(literalValue({ kind: 'float', value: 1 })).toEqual({
      kind: 'literal',
      value: 1,
      float: true,
    });
  });

  it('is held by a parameter as the setting it read, a number with a fraction marked as one', () => {
    expect(literalSetting({ kind: 'integer', value: 12 })).toEqual({ kind: 'number', value: 12 });
    expect(literalSetting({ kind: 'float', value: 2.5 })).toEqual({
      kind: 'number',
      value: 2.5,
      float: true,
    });
    expect(literalSetting({ kind: 'text', value: 'x' })).toEqual({ kind: 'text', value: 'x' });
  });

  it('is shown in words, a whole float with its fraction so that it is not taken for a whole number', () => {
    expect(literalWords({ kind: 'text', value: ' a b ' })).toBe(' a b ');
    expect(literalWords({ kind: 'integer', value: -3 })).toBe('-3');
    expect(literalWords({ kind: 'float', value: 1.5 })).toBe('1.5');
    expect(literalWords({ kind: 'float', value: 2 })).toBe('2.0');
    expect(literalWords({ kind: 'float', value: 1e20 })).toBe('100000000000000000000.0');
    expect(literalWords({ kind: 'float', value: 1e21 })).toBe('1e+21');
  });
});

describe('what a chain’s literal holds', () => {
  it('is shown as the number or text it is, a float marked as one with its fraction, and none as nothing', () => {
    expect(valueWords({ kind: 'literal', value: 3 })).toBe('3');
    expect(valueWords({ kind: 'literal', value: 3, float: true })).toBe('3.0');
    expect(valueWords({ kind: 'literal', value: 'within' })).toBe('within');
    expect(valueWords({ kind: 'literal', value: null })).toBeNull();
  });
});
