/**
 * @file The methods of an expression, in Polars' own vocabulary. Which
 * methods exist, what they are called and what their argument is come from
 * the catalogue the host serves, not from here. What a method does to a
 * value, which dtypes it takes, what dtype it gives and which argument it
 * starts with is the engine's to say, through its offers.
 */

import type { DefaultArgumentView, ParameterView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import { argumentOf, methodEntry } from './catalogue';
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

/** What a method's argument is: its catalogue parameter and the name the method is shown by. */
export interface ArgumentRule {
  readonly name: MethodName;
  /** The name the catalogue shows the method by. */
  readonly shown: string;
  readonly parameter: ParameterView;
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
 * Returns the argument the method `name` starts with when it is added to a
 * chain, or `null` when it takes none. It is the default the engine allowed
 * there, `offered`, a number or text typed in; anything else is still to be
 * picked.
 */
export function startingArgument(
  catalogue: Catalogue,
  name: MethodName,
  offered: DefaultArgumentView | null,
): Value | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  if (offered === null || offered.parameter !== rule.parameter.name) return { kind: 'unset' };
  return { kind: 'literal', value: offered.literal.value };
}

/**
 * Returns the argument `name` has when it replaces a method that had
 * `argument`: the one it had when there is one to keep, and the new method's
 * starting argument otherwise. Whether the new method takes it is the
 * engine's to say when the steps run, and its refusal says why.
 */
export function swappedArgument(
  catalogue: Catalogue,
  argument: Value | null,
  name: MethodName,
  offered: DefaultArgumentView | null,
): Value | null {
  const rule = argumentRule(catalogue, name);
  if (rule === null) return null;
  if (argument === null || argument.kind === 'unset') {
    return startingArgument(catalogue, name, offered);
  }
  return argument;
}
