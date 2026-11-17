/**
 * @file A number or text typed in, as the step document writes it, so that the
 * engine can be asked its dtype. The dtype is the engine's to decide: a whole
 * number and a number with a fraction are not the same, and neither is
 * assumed to be anything here.
 */

import type { LiteralView } from '@dageist/host';

import type { Cell } from './source';

/**
 * Returns what a value written out is, as the document writes it: a whole
 * number that is safe to write as one is an integer, any other number is a
 * float, and text is text. `null` is nothing, which the document cannot hold.
 */
export function literalOf(value: Cell): LiteralView | null {
  if (value === null) return null;
  if (typeof value === 'string') return { kind: 'text', value };
  return Number.isSafeInteger(value) ? { kind: 'integer', value } : { kind: 'float', value };
}

/** Matches a number written as digits, with a fraction if it has one. */
const NUMBER = /^-?\d+(\.\d+)?$/u;

/**
 * Returns the number that `typed` writes out, or `null` when it is not a
 * number. Writing a number is typing, not a rule of a dtype.
 */
export function numberTyped(typed: string): number | null {
  const text = typed.trim();
  if (!NUMBER.test(text)) return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

/** Returns what typing `typed` into a list can give: a number it writes out, and the text itself. */
export function typedValues(typed: string): readonly Cell[] {
  const number = numberTyped(typed);
  return [...(number === null ? [] : [number]), ...(typed === '' ? [] : [typed])];
}
