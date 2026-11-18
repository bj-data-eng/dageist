import { describe, expect, it } from 'vitest';

import { isCondition, isSetting, isValue } from './shape';

describe('the number a window sends another', () => {
  it('may be marked as one the engine read with a fraction, and by nothing else', () => {
    expect(isValue({ kind: 'literal', value: 1, shown: '1' })).toBe(true);
    expect(isValue({ kind: 'literal', value: 1, float: true, shown: '1.0' })).toBe(true);
    expect(isValue({ kind: 'literal', value: 1, float: 'yes', shown: '1.0' })).toBe(false);
    expect(isValue({ kind: 'literal', value: 1, float: false, shown: '1.0' })).toBe(false);
    expect(isSetting({ kind: 'number', value: 1, shown: '1' })).toBe(true);
    expect(isSetting({ kind: 'number', value: 1, float: true, shown: '1.0' })).toBe(true);
    expect(isSetting({ kind: 'number', value: 1, float: 1, shown: '1.0' })).toBe(false);
  });

  it('is finite', () => {
    expect(
      isSetting({ kind: 'number', value: Number.POSITIVE_INFINITY, float: true, shown: 'inf' }),
    ).toBe(false);
  });
});

describe('the words a window sends with a number or text', () => {
  it('are the engine’s, which a number and a text always come with', () => {
    expect(isValue({ kind: 'literal', value: 1 })).toBe(false);
    expect(isValue({ kind: 'literal', value: 'a' })).toBe(false);
    expect(isValue({ kind: 'literal', value: 'a', shown: 1 })).toBe(false);
    expect(isValue({ kind: 'literal', value: 'a', shown: 'a' })).toBe(true);
    expect(isSetting({ kind: 'number', value: 1 })).toBe(false);
    expect(isSetting({ kind: 'text', value: 'a' })).toBe(false);
    expect(isSetting({ kind: 'text', value: 'a', shown: 'a' })).toBe(true);
  });

  it('are not sent with nothing, which has none, nor with a name picked from a list', () => {
    expect(isValue({ kind: 'literal', value: null })).toBe(true);
    expect(isSetting({ kind: 'word', value: 'left' })).toBe(true);
  });
});

/** A test that compares with `comparison`, whatever that is. */
const testComparing = (comparison: unknown): unknown => ({
  kind: 'test',
  id: 't',
  column: 'a',
  comparison,
  value: { kind: 'unset' },
});

describe('the comparison a window sends another', () => {
  it('is the identifier of the entry that compares, or is_in', () => {
    expect(isCondition(testComparing('Expr.eq'))).toBe(true);
    expect(isCondition(testComparing('is_in'))).toBe(true);
    expect(isCondition(testComparing(1))).toBe(false);
    expect(isCondition(testComparing(undefined))).toBe(false);
  });
});
