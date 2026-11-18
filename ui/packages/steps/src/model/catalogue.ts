/**
 * @file The catalogue the host serves, indexed for the step editor: the
 * entry of each method and verb the editor knows.
 *
 * The catalogue owns every word the editor shows. This file only looks them
 * up; it does not know them. Which calls may stand where, and which dtypes
 * an argument may have, is the engine's to say, through its offers.
 */

import type { CatalogueView, EntryView, ParameterView, PositionView } from '@dageist/host';

import { assertNever } from './frame';

/** The catalogue as the editor reads it. */
export interface Catalogue {
  readonly view: CatalogueView;
  readonly entries: ReadonlyMap<string, EntryView>;
}

/** Indexes `view` by entry identifier. */
export function indexCatalogue(view: CatalogueView): Catalogue {
  return {
    view,
    entries: new Map(view.entries.map((entry) => [entry.id, entry])),
  };
}

/** The identifier the catalogue gives the method with Polars name `name`, such as `Expr.dt.date`. */
export function methodId(name: string): string {
  return `Expr.${name}`;
}

/** Returns the Polars name of the expression method with identifier `id`, or `null` for another entry. */
export function methodNameOf(id: string): string | null {
  return id.startsWith('Expr.') ? id.slice('Expr.'.length) : null;
}

/** One comparison a test can make, as the engine words it. */
export interface ComparisonOption {
  /** The entry that compares, which a test holds. */
  readonly entry: string;
  /** The sign the engine writes it with, such as `≥`. */
  readonly sign: string;
  /** What it means, in the entry's own words, such as `at least`. */
  readonly hint: string;
}

/**
 * Returns the comparisons a test can make, in the order the engine lists them, each with its sign
 * and what it means, both the engine's. A comparison whose entry the catalogue lacks is left out.
 */
export function comparisonOptions(catalogue: Catalogue): readonly ComparisonOption[] {
  return catalogue.view.comparisons.flatMap(({ entry, sign }) => {
    const found = catalogue.entries.get(entry);
    return found === undefined ? [] : [{ entry, sign, hint: found.hint }];
  });
}

/** Returns the sign the engine writes the comparison `entry` with, or `null` where it is none. */
export function signOf(catalogue: Catalogue, entry: string): string | null {
  return catalogue.view.comparisons.find((comparison) => comparison.entry === entry)?.sign ?? null;
}

/** The identifier the catalogue gives the step verb `verb`, such as `LazyFrame.filter`. */
export function verbId(verb: string): string {
  return `LazyFrame.${verb}`;
}

/** Returns the catalogue entry of the method called `name`, or `null` when there is none. */
export function methodEntry(catalogue: Catalogue, name: string): EntryView | null {
  return catalogue.entries.get(methodId(name)) ?? null;
}

/** Returns the catalogue entry of the step `verb`, or `null` when there is none. */
export function verbEntry(catalogue: Catalogue, verb: string): EntryView | null {
  return catalogue.entries.get(verbId(verb)) ?? null;
}

/** A method's argument: the first parameter of its entry, when that takes an expression. */
export interface ArgumentParameter {
  readonly parameter: ParameterView;
}

/**
 * Returns the argument of `entry`: its first parameter, when that takes an
 * expression. The other parameters stay at their defaults, and a first
 * parameter of any other kind is not drawn yet.
 */
export function argumentOf(entry: EntryView): ArgumentParameter | null {
  const [first] = entry.parameters;
  if (first?.kind.kind !== 'expression') return null;
  return { parameter: first };
}

/** Returns the position that holds the expression the parameter `name` of `entry` takes, or `null` when no named position does. */
export function positionOfParameter(entry: EntryView | null, name: string): PositionView | null {
  return entry?.parameters.find((parameter) => parameter.name === name)?.position ?? null;
}

/** The words of a condition that the catalogue gives entries to. */
export type ConditionWord = 'when' | 'then' | 'otherwise';

/** The identifier the catalogue gives each word of a condition. */
export function conditionId(word: ConditionWord): string {
  switch (word) {
    case 'when':
      return 'polars.when';
    case 'then':
      return 'When.then';
    case 'otherwise':
      return 'Then.otherwise';
    default:
      return assertNever(word);
  }
}

/** Returns the catalogue entry of the word `word` of a condition, or `null` when there is none. */
export function conditionEntry(catalogue: Catalogue, word: ConditionWord): EntryView | null {
  return catalogue.entries.get(conditionId(word)) ?? null;
}

/** The identifier the catalogue gives `element`, which stands for the values of one cell of a pivot. */
export const ELEMENT_ID = 'polars.element';

/** Returns the catalogue entry of `element`, or `null` when there is none. */
export function elementEntry(catalogue: Catalogue): EntryView | null {
  return catalogue.entries.get(ELEMENT_ID) ?? null;
}
