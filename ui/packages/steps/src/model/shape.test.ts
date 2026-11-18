import { describe, expect, it } from 'vitest';

import { isCondition, isSetting, isValue } from './shape';

/** The entries the catalogue lists as comparisons, which a test of a chain may compare with. */
const COMPARISONS: ReadonlySet<string> = new Set([
  'Expr.eq',
  'Expr.ne',
  'Expr.gt',
  'Expr.ge',
  'Expr.lt',
  'Expr.le',
]);

const holdsValue = (value: unknown): boolean => isValue(value, COMPARISONS);
const holdsSetting = (value: unknown): boolean => isSetting(value, COMPARISONS);
const holdsCondition = (value: unknown): boolean => isCondition(value, COMPARISONS);

describe('the number a window sends another', () => {
  it('may be marked as one the engine read with a fraction, and by nothing else', () => {
    expect(holdsValue({ kind: 'literal', value: 1, shown: '1' })).toBe(true);
    expect(holdsValue({ kind: 'literal', value: 1, float: true, shown: '1.0' })).toBe(true);
    expect(holdsValue({ kind: 'literal', value: 1, float: 'yes', shown: '1.0' })).toBe(false);
    expect(holdsValue({ kind: 'literal', value: 1, float: false, shown: '1.0' })).toBe(false);
    expect(holdsSetting({ kind: 'number', value: 1, shown: '1' })).toBe(true);
    expect(holdsSetting({ kind: 'number', value: 1, float: true, shown: '1.0' })).toBe(true);
    expect(holdsSetting({ kind: 'number', value: 1, float: 1, shown: '1.0' })).toBe(false);
  });

  it('is finite', () => {
    expect(
      holdsSetting({ kind: 'number', value: Number.POSITIVE_INFINITY, float: true, shown: 'inf' }),
    ).toBe(false);
  });
});

describe('the words a window sends with a number or text', () => {
  it('are the engine’s, which a number and a text always come with', () => {
    expect(holdsValue({ kind: 'literal', value: 1 })).toBe(false);
    expect(holdsValue({ kind: 'literal', value: 'a' })).toBe(false);
    expect(holdsValue({ kind: 'literal', value: 'a', shown: 1 })).toBe(false);
    expect(holdsValue({ kind: 'literal', value: 'a', shown: 'a' })).toBe(true);
    expect(holdsSetting({ kind: 'number', value: 1 })).toBe(false);
    expect(holdsSetting({ kind: 'text', value: 'a' })).toBe(false);
    expect(holdsSetting({ kind: 'text', value: 'a', shown: 'a' })).toBe(true);
  });

  it('are not sent with nothing, which has none, nor with a name picked from a list', () => {
    expect(holdsValue({ kind: 'literal', value: null })).toBe(true);
    expect(holdsSetting({ kind: 'word', value: 'left' })).toBe(true);
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
  it('is an entry the catalogue lists as a comparison, or is_in', () => {
    expect(holdsCondition(testComparing('Expr.eq'))).toBe(true);
    expect(holdsCondition(testComparing('Expr.le'))).toBe(true);
    expect(holdsCondition(testComparing('is_in'))).toBe(true);
  });

  it('is refused where it names no comparison, whatever else it is', () => {
    // `==` is how Python spells the comparison, not the entry that compares.
    expect(holdsCondition(testComparing('=='))).toBe(false);
    expect(holdsCondition(testComparing('Expr.nothing'))).toBe(false);
    expect(holdsCondition(testComparing('Expr.shift'))).toBe(false);
    expect(holdsCondition(testComparing(''))).toBe(false);
    expect(holdsCondition(testComparing(1))).toBe(false);
    expect(holdsCondition(testComparing(undefined))).toBe(false);
  });

  it('is checked wherever a condition stands, inside a chain too', () => {
    const chainOf = (comparison: unknown): unknown => ({
      kind: 'chain',
      id: 'c',
      chain: {
        source: {
          kind: 'when',
          arms: [{ id: 'a', condition: testComparing(comparison), returns: { kind: 'unset' } }],
          otherwise: { kind: 'unset' },
        },
        methods: [],
      },
    });
    expect(holdsValue(chainOf('Expr.gt'))).toBe(true);
    expect(holdsValue(chainOf('=='))).toBe(false);
    expect(holdsSetting({ kind: 'expression', value: chainOf('Expr.nothing') })).toBe(false);
  });
});
