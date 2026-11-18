/**
 * @file Chains and what they are made of. A chain starts from a source, a
 * column, a connected input or a condition that picks between values (`when
 * … then … otherwise`), and applies methods to it. A value inside a condition
 * can itself be a chain, so chains nest.
 *
 * What the editor holds here is what the step document is written from. The
 * engine checks it, works out its dtypes and its rows, and writes it as Polars
 * code; nothing here writes text for a person to read.
 */

import type { Method } from './expr';

/** What a typed value is: text, a number, or nothing. */
export type Cell = string | number | null;

/** A value taken from a column, row by row. */
export interface ColumnValue {
  readonly kind: 'column';
  readonly column: string;
}

/** No value: `None`. It has no words, and the engine has no value for it yet. */
export interface NoLiteral {
  readonly kind: 'literal';
  readonly value: null;
}

/** A number or text, the same on every row, with the words the engine shows it as. */
export interface TypedLiteral {
  readonly kind: 'literal';
  readonly value: string | number;
  /** Set for a number the engine read as one with a fraction, which stays one though it is whole. */
  readonly float?: true;
  /** The value as the engine words it, which the editor shows and words no value of its own. */
  readonly shown: string;
}

/** The same value on every row. */
export type LiteralValue = NoLiteral | TypedLiteral;

/** A value worked out by a chain of its own. */
export interface ChainValue {
  readonly kind: 'chain';
  /** Stays with the sub-chain while it is edited. */
  readonly id: string;
  readonly chain: Chain;
}

/** A value that comes from one of the node's connections, by its name. */
export interface InputValue {
  readonly kind: 'input';
  readonly name: string;
}

/** A value that is still to be picked. It stands for nothing until it is. */
export interface UnsetValue {
  readonly kind: 'unset';
}

/** A value a condition compares with or returns. */
export type Value = ColumnValue | LiteralValue | ChainValue | InputValue | UnsetValue;

declare const COMPARISON: unique symbol;

/**
 * What a test compares with: the identifier of an entry the catalogue lists as a comparison, such
 * as `Expr.eq`, whose sign and meaning the engine gives, or `IS_IN`, which tests membership in a
 * list and which the catalogue has no entry for yet. Only those are comparisons: a text is made
 * one by `asComparison`, which checks it against what the catalogue lists.
 */
export type Comparison = string & { readonly [COMPARISON]: true };

/**
 * The comparison that tests membership in a list. The catalogue has no entry for it yet.
 */
// The text is the comparison; the assertion only marks it as the one that was checked.
// oxlint-disable-next-line typescript/no-unsafe-type-assertion
export const IS_IN = 'is_in' as Comparison;

/**
 * Returns `text` as a comparison when it is `IS_IN` or one of the entries in `listed`, which are
 * the entries the catalogue lists as comparisons, and `null` when it is not one.
 */
export function asComparison(text: string, listed: ReadonlySet<string>): Comparison | null {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion
  return text === IS_IN || listed.has(text) ? (text as Comparison) : null;
}

/** One test: `pl.col(column) <comparison> value`. */
export interface Test {
  readonly kind: 'test';
  /** Stays with the test while it is edited or moved into a group. */
  readonly id: string;
  readonly column: string;
  readonly comparison: Comparison;
  readonly value: Value;
}

/**
 * Several conditions taken together. Every one of them must hold, or any one
 * of them is enough. A group inside a group is worked out first, so nothing
 * depends on which of `&` and `|` binds tighter.
 */
export interface Group {
  readonly kind: 'group';
  /** Stays with the group while it is edited. */
  readonly id: string;
  /** Whether every item must hold. Otherwise any one of them is enough. */
  readonly every: boolean;
  readonly items: readonly Condition[];
}

/** A condition written as a chain of its own, which must give a Boolean. */
export interface ChainCondition {
  readonly kind: 'chain';
  /** Stays with the chain while it is edited. */
  readonly id: string;
  readonly chain: Chain;
}

/**
 * What a filter keeps rows by, and what a `when` decides by. A group with no
 * items is a condition that is still to be picked.
 */
export type Condition = Test | Group | ChainCondition;

/** One `.when(condition).then(value)` pair. */
export interface Arm {
  /** Stays with the arm while it is edited. */
  readonly id: string;
  readonly condition: Condition;
  readonly returns: Value;
}

/** A chain that starts from `pl.col(column)`. */
export interface ColumnSource {
  readonly kind: 'column';
  readonly column: string;
}

/** A chain that starts from `pl.when(…).then(…)` pairs and a final `.otherwise(…)`. */
export interface WhenSource {
  readonly kind: 'when';
  readonly arms: readonly Arm[];
  readonly otherwise: Value;
}

/**
 * A chain that starts from `pl.element()`: the values that land in one cell of
 * a pivot, which only the aggregate of a pivot has.
 */
export interface ElementSource {
  readonly kind: 'element';
}

/**
 * A chain that starts from a connected input, such as a list of holidays,
 * which a method then works on: `holidays.len()`.
 */
export interface InputSource {
  readonly kind: 'input';
  readonly name: string;
}

/** What a chain starts from. */
export type Source = ColumnSource | WhenSource | ElementSource | InputSource;

/** A source followed by method calls. */
export interface Chain {
  readonly source: Source;
  readonly methods: readonly Method[];
}
