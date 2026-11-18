/**
 * @file What a parameter holds once it is given: one shape for every kind of
 * parameter the catalogue has, so that one drawing covers them all. A method
 * or a step keeps the parameters that are set in an `Options` record, by the
 * parameter's Python name. A parameter that is not there holds its default,
 * which Polars supplies. What a default is, and whether a setting equals it,
 * is the engine's to say.
 *
 * What a setting stands for in Polars' text needs chains, so that is in
 * `source.ts`; what it stands for in the step document is in `document.ts`.
 */

import type { EntryView, ParameterView } from '@dageist/host';

import { nextId } from './ids';
import type { Value } from './source';

/** One entry of a `pairs` parameter, such as a column and the name it is given. */
export interface PairSetting {
  /** Stays with the entry while it is edited. */
  readonly id: string;
  readonly key: Setting;
  readonly value: Setting;
}

/**
 * What one parameter holds. A parameter that holds many of its kind is a
 * `list`, and one that holds one for each item of another parameter is a
 * single setting for every item or `each`, which names the setting of the
 * items that have one of their own; the others are at the default.
 */
export type Setting =
  /** Still to be given. It stands for nothing until it is. */
  | { readonly kind: 'unset' }
  | { readonly kind: 'flag'; readonly value: boolean }
  /**
   * A number, with the words the engine shows it as. `float` is set for one the engine read as a
   * number with a fraction, which stays one though it is whole.
   */
  | {
      readonly kind: 'number';
      readonly value: number;
      readonly float?: true;
      readonly shown: string;
    }
  /** Text that was typed: a string, a duration or a path, with the words the engine shows it as. */
  | { readonly kind: 'text'; readonly value: string; readonly shown: string }
  /** A name picked from a list: a choice's word, a dtype, a column or a table. */
  | { readonly kind: 'word'; readonly value: string }
  | { readonly kind: 'expression'; readonly value: Value }
  | { readonly kind: 'list'; readonly items: readonly Setting[] }
  | { readonly kind: 'pairs'; readonly entries: readonly PairSetting[] }
  | { readonly kind: 'each'; readonly items: Readonly<Record<string, Setting>> };

/** A setting that holds a number or text typed in. */
export type LiteralSetting = Extract<Setting, { readonly kind: 'number' | 'text' }>;

/** The parameters of a method or step that are set, by Python name. */
export type Options = Readonly<Record<string, Setting>>;

/** Nothing set: every parameter is at its default. */
export const NO_OPTIONS: Options = {};

/** The setting of a parameter that is still to be given. */
export const UNSET: Setting = { kind: 'unset' };

/** Returns `value` as the setting of an expression parameter. */
export function expressionSetting(value: Value): Setting {
  return { kind: 'expression', value };
}

/** Returns a pair with nothing picked yet. */
export function emptyPair(): PairSetting {
  return { id: nextId('pair'), key: UNSET, value: UNSET };
}

/** Reports whether `setting` has everything it needs: nothing in it is still to be given. */
export function isComplete(setting: Setting): boolean {
  switch (setting.kind) {
    case 'unset':
      return false;
    case 'expression':
      return setting.value.kind !== 'unset';
    case 'list':
      return setting.items.length > 0 && setting.items.every((item) => isComplete(item));
    case 'pairs':
      return (
        setting.entries.length > 0 &&
        setting.entries.every((entry) => isComplete(entry.key) && isComplete(entry.value))
      );
    case 'each':
      return Object.values(setting.items).every((item) => isComplete(item));
    default:
      return true;
  }
}

/** One parameter that is set, with what it is set to. */
export interface SetParameter {
  readonly parameter: ParameterView;
  readonly setting: Setting;
}

/** Returns the parameters of `entry` that `options` sets, in the entry's order. */
export function setParameters(entry: EntryView, options: Options): SetParameter[] {
  return entry.parameters.flatMap((parameter) => {
    const setting = Object.hasOwn(options, parameter.name) ? options[parameter.name] : undefined;
    return setting === undefined ? [] : [{ parameter, setting }];
  });
}

/** Returns the parameters of `entry` that a method or step can be given as options: all but `covered`. */
export function optionParameters(
  entry: EntryView,
  covered: readonly string[],
): readonly ParameterView[] {
  return entry.parameters.filter((parameter) => !covered.includes(parameter.name));
}

/** Returns `options` with the parameter called `name` set to `setting`. */
export function withOption(options: Options, name: string, setting: Setting): Options {
  return { ...options, [name]: setting };
}

/** Returns `options` without the parameter called `name`, which is then at its default. */
export function withoutOption(options: Options, name: string): Options {
  return Object.fromEntries(Object.entries(options).filter(([key]) => key !== name));
}

/**
 * Returns the options of a method or step with entry `from` that is swapped
 * for one with entry `to`: those `to` has a parameter of the same name and
 * kind for.
 */
export function carriedOptions(options: Options, from: EntryView | null, to: EntryView): Options {
  return Object.fromEntries(
    Object.entries(options).filter(([name]) => {
      const before = from?.parameters.find((parameter) => parameter.name === name);
      const after = to.parameters.find((parameter) => parameter.name === name);
      return (
        before !== undefined &&
        after !== undefined &&
        JSON.stringify([before.kind, before.shape]) === JSON.stringify([after.kind, after.shape])
      );
    }),
  );
}

/** Returns the setting called `name` when it is set, and `null` otherwise. */
export function optionOf(options: Options, name: string): Setting | null {
  return (Object.hasOwn(options, name) ? options[name] : undefined) ?? null;
}

/** Returns `options` without the parameters called `names`. */
export function withoutOptions(options: Options, names: readonly string[]): Options {
  return Object.fromEntries(Object.entries(options).filter(([key]) => !names.includes(key)));
}

/** Returns the items of the parameter `name` when it holds a list, and no items when it is not set to one. */
export function itemsOf(options: Options, name: string): readonly Setting[] {
  const setting = optionOf(options, name);
  return setting?.kind === 'list' ? setting.items : [];
}

/** Returns the entries of the parameter `name` when it holds pairs, and none when it is not set to them. */
export function pairsOf(options: Options, name: string): readonly PairSetting[] {
  const setting = optionOf(options, name);
  return setting?.kind === 'pairs' ? setting.entries : [];
}

/** Returns the word or text `setting` holds, or `null` when it holds neither. */
export function textOf(setting: Setting): string | null {
  return setting.kind === 'word' || setting.kind === 'text' ? setting.value : null;
}
