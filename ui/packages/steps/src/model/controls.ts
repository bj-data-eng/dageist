/**
 * @file Which control each parameter of the catalogue gets. The components draw a control and
 * decide nothing about it: this file says, from the parameter's kind and shape and from the entry
 * it belongs to, whether the editor draws it as a field, a typed value, a flag, a choice, a list of
 * these, entries of pairs or one value for each item of another parameter, and whether it draws
 * the parameter at all. A parameter that gets no control is a gap, which the list of declared gaps
 * in [limits.ts](./limits.ts) names.
 */

import type { ChoiceView, EntryView, KindView, NoneView, ParameterView } from '@dageist/host';

import { argumentOf } from './catalogue';

/**
 * What is typed into a literal: a number of a stated type, text, a duration, a pattern, a path or
 * a date.
 */
export type LiteralType =
  | 'integer'
  | 'float'
  | 'string'
  | 'date'
  | 'any'
  | 'duration'
  | 'pattern'
  | 'timeZone'
  | 'path';

/** How one argument of one kind is drawn. */
export type OneControl =
  /** A chain, a connected input, a field or a typed value, in the slot of a value. */
  | { readonly control: 'expression' }
  /** One field of the rows. */
  | { readonly control: 'column' }
  /** Something typed into the search box of its list. */
  | { readonly control: 'literal'; readonly type: LiteralType }
  | { readonly control: 'flag' }
  | { readonly control: 'choice'; readonly words: readonly ChoiceView[] }
  | { readonly control: 'dtype' }
  | { readonly control: 'timeZone'; readonly none: NoneView | null }
  /** A table connected to the node. */
  | { readonly control: 'table' }
  /** No drawing: the words say why. */
  | { readonly control: 'none'; readonly why: string };

/** Returns how an argument of `kind` is drawn on its own: every kind but a pair of pairs has a drawing. */
export function oneControlOf(kind: KindView): OneControl {
  switch (kind.kind) {
    case 'expression':
      return { control: 'expression' };
    case 'column':
      return { control: 'column' };
    case 'value':
      return { control: 'literal', type: kind.type };
    case 'duration':
    case 'pattern':
    case 'path':
      return { control: 'literal', type: kind.kind };
    case 'flag':
      return { control: 'flag' };
    case 'choice':
      return { control: 'choice', words: kind.words };
    case 'dtype':
      return { control: 'dtype' };
    case 'timeZone':
      return { control: 'timeZone', none: kind.none };
    case 'table':
      return { control: 'table' };
    case 'pairs':
      // Entries are one kind each, so a pair does not hold pairs.
      return { control: 'none', why: 'pairs inside pairs are not drawn' };
    default:
      return kind satisfies never;
  }
}

/** How a parameter is drawn, by its shape and kind. */
export type ParameterControl =
  /** One argument of one kind. */
  | { readonly shape: 'one'; readonly one: OneControl }
  /** Several fields, folded to the room on the line. */
  | { readonly shape: 'columns' }
  /** Several arguments of one kind, in order, each of which can be taken away. */
  | { readonly shape: 'many'; readonly each: OneControl }
  /** The entries of pairs: a key of one kind beside a value of another. */
  | { readonly shape: 'pairs'; readonly key: OneControl; readonly value: OneControl }
  /** One value for each item of another parameter, chosen from a short list. */
  | { readonly shape: 'perItem'; readonly each: 'choose' }
  /** One value for each item of another parameter, of a kind that is not chosen from a short list. */
  | { readonly shape: 'perItem'; readonly each: 'none'; readonly why: string };

/**
 * Returns how `parameter` is drawn: a parameter that holds one value for each item of another is
 * chosen from a list, pairs are entries, several of a kind are a list, and anything else is one
 * argument of its kind.
 */
export function controlOf(parameter: ParameterView): ParameterControl {
  const { kind, shape } = parameter;
  if (shape.shape === 'perItem') {
    // What a value takes when it is typed is read by the engine in a list of its own, which a row
    // of a list has no room for, so a parameter of another kind is not drawn here.
    return kind.kind === 'flag' || kind.kind === 'choice'
      ? { shape: 'perItem', each: 'choose' }
      : { shape: 'perItem', each: 'none', why: `${parameter.shown} is not drawn here` };
  }
  if (kind.kind === 'pairs') {
    return { shape: 'pairs', key: oneControlOf(kind.key), value: oneControlOf(kind.value) };
  }
  if (shape.shape === 'many') {
    return kind.kind === 'column'
      ? { shape: 'columns' }
      : { shape: 'many', each: oneControlOf(kind) };
  }
  return { shape: 'one', one: oneControlOf(kind) };
}

/**
 * Returns why a parameter of `kind` that is added as an option has to wait for the engine to say
 * what it starts with: a flag and a choice start with the word the engine's table of options gives,
 * and every other kind is added by asking the engine to start it.
 */
export function startsWithEngineWord(kind: KindView): boolean {
  return kind.kind === 'flag' || kind.kind === 'choice';
}

/**
 * The parameters the editor sets of an entry that it draws as a word of its own, so that it does not
 * set the others. An entry that is not here has every parameter set where its parameters are drawn.
 */
const SET_BY_THE_ENTRY: Readonly<Record<string, readonly string[]>> = {
  // A field is picked by its name; several names at once are not drawn.
  'polars.col': ['name'],
  // A number or text is typed in; a literal of a given dtype is not drawn.
  'polars.lit': ['value'],
  // A condition is its predicates; keyword constraints are not drawn.
  'polars.when': ['predicates'],
  'Then.when': ['predicates'],
  'ChainedThen.when': ['predicates'],
};

/** How the editor gives one parameter of an entry its value. */
export type Setting =
  /** The entry's own drawing sets it: a field's name, a typed value, a condition's predicates. */
  | { readonly by: 'entry' }
  /** The slot beside a method's name, which takes the method's first parameter when it is an expression. */
  | { readonly by: 'argument'; readonly control: ParameterControl }
  /** An option of the method or the step, added from the list of its name. */
  | { readonly by: 'option'; readonly control: ParameterControl }
  /** The engine does not offer it: it is in Python's signature and changes nothing here. */
  | { readonly by: 'unoffered' }
  /** The entry's drawing does not set it, though the engine offers it. */
  | { readonly by: 'unset' };

/**
 * Returns how the editor sets `parameter` of `entry`: by the entry's own word where the entry is
 * drawn as one, otherwise as the argument of a method, an option, or not at all where the engine
 * does not offer it.
 */
export function settingOf(entry: EntryView, parameter: ParameterView): Setting {
  const set = Object.hasOwn(SET_BY_THE_ENTRY, entry.id) ? SET_BY_THE_ENTRY[entry.id] : undefined;
  if (set !== undefined) return set.includes(parameter.name) ? { by: 'entry' } : { by: 'unset' };
  if (!parameter.offered) return { by: 'unoffered' };
  const control = controlOf(parameter);
  return argumentOf(entry)?.parameter.name === parameter.name
    ? { by: 'argument', control }
    : { by: 'option', control };
}

/**
 * Returns the parameters of the method `entry` that can be added to it as options: those the engine
 * offers, all but the argument that is drawn where the method is.
 */
export function methodOptionParameters(entry: EntryView | null): readonly ParameterView[] {
  if (entry === null) return [];
  return entry.parameters.filter((parameter) => settingOf(entry, parameter).by === 'option');
}
