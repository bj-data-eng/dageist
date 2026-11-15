/**
 * @file The catalogue the host serves, indexed for the step editor: the
 * entry of each method and verb the storyboard knows, and the words for the
 * dtype groups an entry accepts.
 *
 * The catalogue owns every word the editor shows and which dtypes a method
 * takes. This file only looks them up; it does not know them.
 */

import type { CatalogueView, EntryView, ParameterView } from '@dageist/host';

import type { Dtype } from './frame';
import { assertNever, isNumeric, isTemporal } from './frame';

/** The catalogue as the editor reads it. */
export interface Catalogue {
  readonly view: CatalogueView;
  readonly entries: ReadonlyMap<string, EntryView>;
  readonly dtypeTitles: ReadonlyMap<string, string>;
}

/** Indexes `view` by entry identifier and dtype group. */
export function indexCatalogue(view: CatalogueView): Catalogue {
  return {
    view,
    entries: new Map(view.entries.map((entry) => [entry.id, entry])),
    dtypeTitles: new Map(view.dtypes.map((words) => [words.key, words.title])),
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
  /** The dtype groups the argument may have. */
  readonly dtypes: readonly string[];
}

/**
 * Returns the argument of `entry`: its first parameter, when that takes an
 * expression. The other parameters stay at their defaults, and a first
 * parameter of any other kind is not drawn yet.
 */
export function argumentOf(entry: EntryView): ArgumentParameter | null {
  const [first] = entry.parameters;
  if (first?.kind.kind !== 'expression') return null;
  return { parameter: first, dtypes: first.kind.dtypes };
}

/**
 * Reports whether `dtype` belongs to the dtype group `group`. A group is
 * every dtype, the numbers, the integers, the temporal dtypes, or one dtype
 * by its name. The sample has only integers among its numbers.
 */
export function groupHolds(group: string, dtype: Dtype): boolean {
  switch (group) {
    case 'any':
      return true;
    case 'numeric':
    case 'integer':
      return isNumeric(dtype);
    case 'temporal':
      return isTemporal(dtype);
    default:
      return group === dtype;
  }
}

/** Reports whether `dtype` belongs to any of `groups`. */
export function groupsHold(groups: readonly string[], dtype: Dtype): boolean {
  return groups.some((group) => groupHolds(group, dtype));
}

/** Returns the words for `groups`, such as "numeric or Duration", from the catalogue. */
export function groupWords(catalogue: Catalogue, groups: readonly string[]): string {
  const titles = groups.map((group) => catalogue.dtypeTitles.get(group) ?? group);
  const last = titles.at(-1);
  if (last === undefined) return 'nothing';
  return titles.length === 1 ? last : `${titles.slice(0, -1).join(', ')} or ${last}`;
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
