/**
 * @file The methods of an expression, in Polars' own vocabulary. Which
 * methods exist, what they are called, which dtypes they take and what their
 * argument is come from the catalogue the host serves, not from here. What a
 * method does to a value, and what dtype it gives, is the engine's to say.
 */

import type { EntryView, ParameterView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import {
  argumentOf,
  groupWords,
  groupsHold,
  methodEntry,
  methodId,
  methodNameOf,
} from './catalogue';
import type { Dtype, Frame } from './frame';
import { findColumn } from './frame';
import type { Options } from './settings';
import { optionParameters } from './settings';
import type { Value } from './source';

/** A method by its Polars name, such as `shift` or `dt.date`. */
export type MethodName = string;

/**
 * One method call. `argument` is the method's first parameter when that takes
 * an expression, as the value it is given, and `null` for a method that takes
 * none. Any other parameter the method takes is in `options` when it is set.
 */
export interface Method {
  /** Stays with the call while it is moved or edited. */
  readonly id: string;
  readonly name: MethodName;
  readonly argument: Value | null;
  readonly options: Options;
}

/** An expression method the catalogue offers. */
export interface OfferedMethod {
  readonly name: MethodName;
  readonly entry: EntryView;
}

/** Returns the expression entries of `catalogue`, in the catalogue's order. */
export function offeredMethods(catalogue: Catalogue): OfferedMethod[] {
  return catalogue.view.entries.flatMap((entry) => {
    const name = entry.applies === 'expression' ? methodNameOf(entry.id) : null;
    return name === null ? [] : [{ name, entry }];
  });
}

/** Why something does not fit: briefly, for a list, and as a sentence. */
export interface Misfit {
  readonly short: string;
  readonly message: string;
}

/**
 * Returns why the method `name` is not offered a value of `input` dtype, or
 * `null` when it is. It only follows what the catalogue says the method
 * accepts; whether a call may stand is the engine's to say.
 */
export function acceptanceMisfit(
  catalogue: Catalogue,
  name: MethodName,
  input: Dtype,
): Misfit | null {
  const entry = methodEntry(catalogue, name);
  if (entry === null) {
    return {
      short: 'not in the catalogue',
      message: `The catalogue has no entry ${methodId(name)}, so ${name} cannot be used.`,
    };
  }
  if (!groupsHold(entry.accepts, input)) {
    const words = groupWords(catalogue, entry.accepts);
    return {
      short: `needs ${words}`,
      message: `${entry.name} needs ${words}, but it is given ${input}.`,
    };
  }
  return null;
}

/** What a method's argument is held to: its catalogue parameter and the name the method is shown by. */
export interface ArgumentRule {
  readonly name: MethodName;
  /** The name the catalogue shows the method by. */
  readonly shown: string;
  readonly parameter: ParameterView;
  /** The dtype groups the argument may have. */
  readonly dtypes: readonly string[];
}

/**
 * Returns the rule of the argument of the method `name`, or `null` when the
 * catalogue has no entry for it or its first parameter is not an expression.
 */
export function argumentRule(catalogue: Catalogue, name: MethodName): ArgumentRule | null {
  const entry = methodEntry(catalogue, name);
  const argument = entry === null ? null : argumentOf(entry);
  if (entry === null || argument === null) return null;
  return { name, shown: entry.name, ...argument };
}

/**
 * Returns the parameters of the method `name` that can be added to it: all
 * but the argument that is drawn where the method is.
 */
export function methodOptionParameters(
  catalogue: Catalogue,
  name: MethodName,
): readonly ParameterView[] {
  const entry = methodEntry(catalogue, name);
  if (entry === null) return [];
  const argument = argumentOf(entry);
  return optionParameters(entry, argument === null ? [] : [argument.parameter.name]);
}

/**
 * Returns why an argument of `dtype` is not taken by the parameter `rule`
 * describes, or `null` when the catalogue's dtype groups take it. It sorts
 * what a list offers; whether a call may stand is the engine's to say.
 */
export function argumentReason(
  rule: ArgumentRule,
  catalogue: Catalogue,
  dtype: Dtype,
): string | null {
  return groupsHold(rule.dtypes, dtype) ? null : `needs ${groupWords(catalogue, rule.dtypes)}`;
}

/** The dtype a whole number written on its own is offered as, and text. */
export const LITERAL_DTYPES = { number: 'Int64', text: 'String' } as const;

/** Returns the dtype of `argument` where it is known without the engine: a field's, a number's, text. */
function dtypeOf(argument: Value, frame: Frame): Dtype | null {
  if (argument.kind === 'column') return findColumn(frame, argument.column)?.dtype ?? null;
  if (argument.kind !== 'literal' || argument.value === null) return null;
  return typeof argument.value === 'number' ? LITERAL_DTYPES.number : LITERAL_DTYPES.text;
}

/**
 * Returns the argument `name` starts with when it is added to a chain, or
 * `null` when it takes none. A default Polars gives as a whole number or as
 * text is that value, when it fits the parameter. Anything else is still to
 * be picked.
 */
export function defaultArgument(catalogue: Catalogue, name: MethodName): Value | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  const given = rule.parameter.default;
  if (given.kind === 'integer' && Number.isSafeInteger(given.value)) {
    return groupsHold(rule.dtypes, LITERAL_DTYPES.number)
      ? { kind: 'literal', value: given.value }
      : { kind: 'unset' };
  }
  if (given.kind === 'text') {
    return groupsHold(rule.dtypes, LITERAL_DTYPES.text)
      ? { kind: 'literal', value: given.value }
      : { kind: 'unset' };
  }
  return { kind: 'unset' };
}

/**
 * Returns the argument `name` has when it replaces `method`'s: the one the
 * method had when the catalogue says the new parameter takes its dtype, and
 * the default otherwise. An argument whose dtype is not known here is kept.
 */
export function swappedArgument(
  catalogue: Catalogue,
  argument: Value | null,
  name: MethodName,
  frame: Frame,
): Value | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  if (argument === null || argument.kind === 'unset') return defaultArgument(catalogue, name);
  const dtype = dtypeOf(argument, frame);
  const fits = dtype === null || groupsHold(rule.dtypes, dtype);
  return fits ? argument : defaultArgument(catalogue, name);
}
