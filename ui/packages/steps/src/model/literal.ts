/**
 * @file A number or text the engine read from what was typed, as the editor
 * holds it. The engine decides what a text is, which dtype it has and the words
 * it is shown as; what is here only carries that answer into a value of a chain
 * or the setting of a parameter.
 */

import type { LiteralView } from '@dageist/host';

import type { LiteralSetting } from './settings';
import type { TypedLiteral } from './source';

/**
 * Returns the value of a chain that holds `literal`, with the words the engine shows it as. A
 * number the engine read with a fraction stays one though it is whole, so that it is written as
 * one and has the dtype the engine said it has.
 */
export function literalValue(literal: LiteralView): TypedLiteral {
  switch (literal.kind) {
    case 'integer':
    case 'text':
      return { kind: 'literal', value: literal.value, shown: literal.shown };
    case 'float':
      return { kind: 'literal', value: literal.value, float: true, shown: literal.shown };
    default:
      return literal satisfies never;
  }
}

/** Returns the setting of a parameter that holds `literal`, which keeps a float a float as a chain's value does. */
export function literalSetting(literal: LiteralView): LiteralSetting {
  switch (literal.kind) {
    case 'integer':
      return { kind: 'number', value: literal.value, shown: literal.shown };
    case 'float':
      return { kind: 'number', value: literal.value, float: true, shown: literal.shown };
    case 'text':
      return { kind: 'text', value: literal.value, shown: literal.shown };
    default:
      return literal satisfies never;
  }
}
