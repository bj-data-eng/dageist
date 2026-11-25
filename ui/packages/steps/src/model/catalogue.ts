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
  /** The entries that compare, which a test may hold: those the engine lists that the catalogue has. */
  readonly comparisons: ReadonlySet<string>;
}

/** Indexes `view` by entry identifier. */
export function indexCatalogue(view: CatalogueView): Catalogue {
  const entries = new Map(view.entries.map((entry) => [entry.id, entry]));
  return {
    view,
    entries,
    comparisons: new Set(
      view.comparisons.flatMap(({ entry }) => (entries.has(entry) ? [entry] : [])),
    ),
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
  /** The entry that compares, such as `Expr.eq`. */
  readonly entry: string;
  /** The sign the engine writes it with, such as `≥`. */
  readonly sign: string;
  /** What it means, in the entry's own words, such as `at least`. */
  readonly hint: string;
}

/**
 * Returns the comparisons the engine lists, in the order it lists them, each with its sign and what
 * it means, both the engine's. A comparison whose entry the catalogue lacks is left out.
 */
export function comparisonOptions(catalogue: Catalogue): readonly ComparisonOption[] {
  return catalogue.view.comparisons.flatMap(({ entry, sign }) => {
    const found = catalogue.entries.get(entry);
    return found === undefined ? [] : [{ entry, sign, hint: found.hint }];
  });
}

/**
 * Returns the option a test that compares with `entry` shows: its sign and what it means, or the
 * entry's own name for one the engine does not list as a comparison.
 */
export function comparisonOption(catalogue: Catalogue, entry: string): ComparisonOption {
  return (
    comparisonOptions(catalogue).find((option) => option.entry === entry) ?? {
      entry,
      sign: entry,
      hint: catalogue.entries.get(entry)?.hint ?? 'not in the catalogue',
    }
  );
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

/** The functions the editor starts a chain from besides a field, an input and a value typed in. */
export const START_FUNCTIONS: ReadonlySet<string> = new Set([conditionId('when'), ELEMENT_ID]);

/** Returns the catalogue entry of `element`, or `null` when there is none. */
export function elementEntry(catalogue: Catalogue): EntryView | null {
  return catalogue.entries.get(ELEMENT_ID) ?? null;
}

/** What a method's argument is: its catalogue parameter and the name the method is shown by. */
export interface ArgumentRule {
  readonly name: string;
  /** The name the catalogue shows the method by. */
  readonly shown: string;
  readonly parameter: ParameterView;
}

/**
 * Returns the rule of the argument of the method `entry`, or `null` when its first parameter is not
 * an expression.
 */
export function argumentRule(entry: EntryView | null): ArgumentRule | null {
  const argument = entry === null ? null : argumentOf(entry);
  if (entry === null || argument === null) return null;
  return { name: entry.id, shown: entry.name, ...argument };
}

/** Returns the parameters of `entry` that can be added to a call as options: those the engine offers, but `covered`. */
export function optionParameters(
  entry: EntryView,
  covered: readonly string[],
): readonly ParameterView[] {
  return entry.parameters.filter(
    (parameter) => parameter.offered && !covered.includes(parameter.name),
  );
}

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'filter' | 'sort' | 'select' | 'rename' | 'join' | 'pivot';

/** Every verb the editor offers, in the order it is offered. */
export const VERBS: readonly Verb[] = [
  'with_columns',
  'filter',
  'sort',
  'select',
  'rename',
  'join',
  'pivot',
];

/**
 * The parameters each step's head draws, by the name each is given here, which the head's
 * drawing reads: the head of a step is laid out by the step, and the other parameters are
 * options added from the step's name.
 */
export const HEAD = {
  // A column and what it holds are the step's own structure.
  with_columns: { named: 'named_exprs', exprs: 'exprs' },
  filter: { predicates: 'predicates' },
  sort: { by: 'by' },
  select: { exprs: 'exprs' },
  rename: { mapping: 'mapping' },
  join: { other: 'other', leftOn: 'left_on', rightOn: 'right_on' },
  pivot: { on: 'on', index: 'index', values: 'values', aggregate: 'aggregate_function' },
} as const satisfies Readonly<Record<Verb, Readonly<Record<string, string>>>>;

/** The name of a parameter a step's head draws: one of the names `HEAD` gives. */
export type HeadName = {
  readonly [Step in Verb]: (typeof HEAD)[Step][keyof (typeof HEAD)[Step]];
}[Verb];

/**
 * The parameter that holds the tests of a condition. A filter's head and a `when` draw the same
 * blocks of conditions and name the parameter alike, so `when` reads the name from the one `HEAD`
 * gives the filter, and the two cannot come apart.
 */
export const CONDITION_TESTS: HeadName = HEAD.filter.predicates;

/**
 * The parameters a step's head does not draw and that are not offered as options either: the
 * fields to order by are one list, so a second one for more fields is not offered, and the keys of
 * the two tables are one list each, so `on` for a shared key is not offered.
 */
const NOT_OFFERED: Readonly<Record<Verb, readonly string[]>> = {
  with_columns: [],
  filter: [],
  sort: ['more_by'],
  select: [],
  rename: [],
  join: ['on'],
  pivot: [],
};

/** Returns the parameters the step `verb` draws in its head or does not offer as options. */
function coveredParameters(verb: Verb): readonly string[] {
  return [...Object.values(HEAD[verb]), ...NOT_OFFERED[verb]];
}

/** Returns the verb a step of the entry `entry` is, or `null` for a step the editor draws by its parameters alone. */
export function verbOf(entry: string): Verb | null {
  return VERBS.find((verb) => verbId(verb) === entry) ?? null;
}

/** Returns the parameters of the step `entry` that can be added to it as options. */
export function stepOptionParameters(
  catalogue: Catalogue,
  entry: string,
): readonly ParameterView[] {
  const found = catalogue.entries.get(entry);
  if (found === undefined) return [];
  const verb = verbOf(entry);
  return optionParameters(found, verb === null ? [] : coveredParameters(verb));
}
