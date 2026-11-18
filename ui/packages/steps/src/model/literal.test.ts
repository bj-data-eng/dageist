import { describe, expect, it } from 'vitest';

import { literalSetting, literalValue } from './literal';

describe('a number or text the engine read', () => {
  it('is held by a chain as the value it read, with the words the engine shows it as', () => {
    expect(literalValue({ kind: 'integer', value: 12, shown: '12' })).toEqual({
      kind: 'literal',
      value: 12,
      shown: '12',
    });
    expect(literalValue({ kind: 'text', value: '12', shown: '12' })).toEqual({
      kind: 'literal',
      value: '12',
      shown: '12',
    });
  });

  it('is held by a chain as a number with a fraction, marked as one, though it is whole', () => {
    // A whole number read with a fraction stays a float: it is not an integer for being whole,
    // and the engine's words for it keep the fraction.
    expect(literalValue({ kind: 'float', value: 1, shown: '1.0' })).toEqual({
      kind: 'literal',
      value: 1,
      float: true,
      shown: '1.0',
    });
  });

  it('is held by a parameter as the setting it read, with the words the engine shows it as', () => {
    expect(literalSetting({ kind: 'integer', value: 12, shown: '12' })).toEqual({
      kind: 'number',
      value: 12,
      shown: '12',
    });
    expect(literalSetting({ kind: 'float', value: 2, shown: '2.0' })).toEqual({
      kind: 'number',
      value: 2,
      float: true,
      shown: '2.0',
    });
    expect(literalSetting({ kind: 'float', value: 1e20, shown: '1e+20' })).toEqual({
      kind: 'number',
      value: 1e20,
      float: true,
      shown: '1e+20',
    });
    expect(literalSetting({ kind: 'text', value: ' a b ', shown: ' a b ' })).toEqual({
      kind: 'text',
      value: ' a b ',
      shown: ' a b ',
    });
  });
});
