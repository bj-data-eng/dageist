/**
 * @file A number or text the engine read from what was typed, as the editor
 * holds it and shows it. The engine decides what a text is and which dtype it
 * has; what is here only carries that answer into a value of a chain or the
 * setting of a parameter, and writes it out in words.
 */

import type { LiteralView } from '@dageist/host';

import type { LiteralSetting } from './settings';
import type { LiteralValue } from './source';

/**
 * Returns the value of a chain that holds `literal`. A number the engine
 * read with a fraction stays one though it is whole, so that it is written as
 * one and has the dtype the engine said it has.
 */
export function literalValue(literal: LiteralView): LiteralValue {
  switch (literal.kind) {
    case 'integer':
    case 'text':
      return { kind: 'literal', value: literal.value };
    case 'float':
      return { kind: 'literal', value: literal.value, float: true };
    default:
      return literal satisfies never;
  }
}

/** Returns the setting of a parameter that holds `literal`, which keeps a float a float as a chain's value does. */
export function literalSetting(literal: LiteralView): LiteralSetting {
  switch (literal.kind) {
    case 'integer':
      return { kind: 'number', value: literal.value };
    case 'float':
      return { kind: 'number', value: literal.value, float: true };
    case 'text':
      return { kind: 'text', value: literal.value };
    default:
      return literal satisfies never;
  }
}

/**
 * Returns `literal` as it is shown: text as it is, and a number as digits,
 * a whole float with the fraction it was read with, so that it is not taken
 * for a whole number.
 */
export function literalWords(literal: LiteralView): string {
  switch (literal.kind) {
    case 'text':
      return literal.value;
    case 'integer':
      return String(literal.value);
    case 'float':
      return Number.isInteger(literal.value) && Math.abs(literal.value) < 1e21
        ? `${String(literal.value)}.0`
        : String(literal.value);
    default:
      return literal satisfies never;
  }
}

/**
 * Returns the number or text a chain's literal holds as it is shown, or
 * `null` for nothing. A number the engine read with a fraction is shown with
 * it, as `literalWords` shows it.
 */
export function valueWords(value: LiteralValue): string | null {
  if (value.value === null) return null;
  if (typeof value.value === 'string') return value.value;
  return literalWords({ kind: value.float === true ? 'float' : 'integer', value: value.value });
}
