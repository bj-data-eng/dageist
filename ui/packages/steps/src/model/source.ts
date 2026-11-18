/**
 * @file Chains and what they are made of. A chain starts from a source, a
 * column, a connected input or a condition that picks between values (`when
 * … then … otherwise`), and applies methods to it. A value inside a condition
 * can itself be a chain, so chains nest.
 *
 * What the editor holds here is what the step document is written from. The
 * engine checks it, works out its dtypes and its rows; this file only writes
 * it out as Polars code, for a person to read.
 */

import type { EntryView, KindView, ParameterView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import { methodEntry } from './catalogue';
import type { Method } from './expr';
import { assertNever, quoted } from './frame';
import type { Options, Setting } from './settings';
import { atDefaultOver, defaultSetting, isComplete, setParameters } from './settings';

/** What a value written out is: text, a number, or nothing. */
export type Cell = string | number | null;

/** Returns `value` as Python would write it. */
export function literalText(value: Cell): string {
  if (value === null) return 'None';
  return typeof value === 'number' ? String(value) : quoted(value);
}

/** A value taken from a column, row by row. */
export interface ColumnValue {
  readonly kind: 'column';
  readonly column: string;
}

/** The same value on every row. */
export interface LiteralValue {
  readonly kind: 'literal';
  readonly value: Cell;
  /** Set for a number the engine read as one with a fraction, which stays one though it is whole. */
  readonly float?: true;
}

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

/** The comparisons, as Polars and Python spell them. `is_in` tests membership in a list. */
export type Comparison = '==' | '!=' | '>' | '>=' | '<' | '<=' | 'is_in';

/** Every comparison, in the order it is offered. */
export const COMPARISONS: readonly Comparison[] = ['==', '!=', '>', '>=', '<', '<=', 'is_in'];

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

/** Returns the Polars text of `value` where a column or expression is expected. */
export function valueText(catalogue: Catalogue, value: Value): string {
  if (value.kind === 'chain') return chainText(catalogue, value.chain);
  // What is still to be picked is written as `None`, which is what Polars would be given.
  if (value.kind === 'unset') return 'None';
  // An input is named in the code as it is in the editor.
  if (value.kind === 'input') return value.name;
  return value.kind === 'column' ? `pl.col(${quoted(value.column)})` : literalText(value.value);
}

/** Returns the Polars text of `value` as an expression, where a bare value would not be one. */
export function valueExprText(catalogue: Catalogue, value: Value): string {
  return value.kind === 'column' || value.kind === 'chain'
    ? valueText(catalogue, value)
    : `pl.lit(${valueText(catalogue, value)})`;
}

/**
 * Returns the Polars text of `value` as a key, such as a field to sort by: a
 * field is its name, which is how Python takes a column, and anything else is
 * an expression.
 */
export function keyText(catalogue: Catalogue, value: Value): string {
  return value.kind === 'column' ? quoted(value.column) : valueExprText(catalogue, value);
}

/** Returns the Polars text of `condition`. */
export function conditionText(catalogue: Catalogue, condition: Condition): string {
  if (condition.kind === 'group') {
    // Every item stands in brackets, so the text says what the lists say.
    const joined = condition.items.map((item) => `(${conditionText(catalogue, item)})`);
    return joined.join(condition.every ? ' & ' : ' | ');
  }
  if (condition.kind === 'chain') return chainText(catalogue, condition.chain);
  if (condition.comparison === 'is_in') {
    return `pl.col(${quoted(condition.column)}).is_in(${valueText(catalogue, condition.value)})`;
  }
  return `pl.col(${quoted(condition.column)}) ${condition.comparison} ${valueText(catalogue, condition.value)}`;
}

/** Returns the Polars text of a value passed to `then` or `otherwise`. */
function returnedText(catalogue: Catalogue, value: Value): string {
  // A bare string there would be read as a column name.
  return value.kind === 'literal' && typeof value.value === 'string'
    ? `pl.lit(${quoted(value.value)})`
    : valueText(catalogue, value);
}

/** Returns the Polars text `source` starts a chain with. */
function sourceText(catalogue: Catalogue, source: Source): string {
  if (source.kind === 'column') return `pl.col(${quoted(source.column)})`;
  if (source.kind === 'element') return 'pl.element()';
  // An input is named in the code as it is in the editor.
  if (source.kind === 'input') return source.name;
  const arms = source.arms
    .map(
      (arm) =>
        `.when(${conditionText(catalogue, arm.condition)}).then(${returnedText(catalogue, arm.returns)})`,
    )
    .join('');
  return `pl${arms}.otherwise(${returnedText(catalogue, source.otherwise)})`;
}

/** Returns the Polars text of `chain`. */
export function chainText(catalogue: Catalogue, chain: Chain): string {
  const methods = chain.methods.map((method) => methodCall(catalogue, method));
  return `${sourceText(catalogue, chain.source)}${methods.join('')}`;
}

/** Returns the Polars text of an expression given as an argument: a field, a number or text, a chain. */
function argumentValueText(catalogue: Catalogue, value: Value): string {
  if (value.kind === 'literal' && typeof value.value === 'string') {
    return `pl.lit(${quoted(value.value)})`;
  }
  return valueText(catalogue, value);
}

/** Returns the Polars text of `setting` as a parameter of `kind` holds it. */
function settingText(
  catalogue: Catalogue,
  kind: KindView,
  setting: Setting,
  items: readonly string[],
  parameter: ParameterView,
): string {
  switch (setting.kind) {
    case 'unset':
      return 'None';
    case 'flag':
      return setting.value ? 'True' : 'False';
    case 'number':
      return String(setting.value);
    case 'text':
      return quoted(setting.value);
    case 'word':
      if (kind.kind === 'dtype') return `pl.${setting.value}`;
      return kind.kind === 'table' ? setting.value : quoted(setting.value);
    case 'expression':
      return argumentValueText(catalogue, setting.value);
    case 'list':
      return `[${setting.items.map((item) => settingText(catalogue, kind, item, items, parameter)).join(', ')}]`;
    case 'pairs': {
      const key = kind.kind === 'pairs' ? kind.key : kind;
      const value = kind.kind === 'pairs' ? kind.value : kind;
      const pairs = setting.entries.map(
        (entry) =>
          `${settingText(catalogue, key, entry.key, [], parameter)}: ${settingText(catalogue, value, entry.value, [], parameter)}`,
      );
      return `{${pairs.join(', ')}}`;
    }
    case 'each': {
      // Every item has its place in the list; one with no setting of its own is at the default.
      const fallback = defaultSetting(parameter);
      const each = items.map((item) => {
        const own = Object.hasOwn(setting.items, item) ? setting.items[item] : undefined;
        const one = own ?? fallback;
        return one === null || one === undefined
          ? 'None'
          : settingText(catalogue, kind, one, [], parameter);
      });
      return `[${each.join(', ')}]`;
    }
    default:
      return assertNever(setting);
  }
}

/** Returns the Polars text of what `parameter` holds when it is set to `setting`, without its name. */
export function settingValueText(
  catalogue: Catalogue,
  parameter: ParameterView,
  setting: Setting,
  items: readonly string[],
): string {
  return settingText(catalogue, parameter.kind, setting, items, parameter);
}

/**
 * Returns how `parameter` is written in a call when it is set to `setting`.
 * Extra positional arguments are written as they are and extra keyword
 * arguments as a dictionary of them; anything else is named.
 */
function parameterText(
  catalogue: Catalogue,
  parameter: ParameterView,
  setting: Setting,
  items: readonly string[],
): string {
  const text = settingText(catalogue, parameter.kind, setting, items, parameter);
  if (parameter.default.kind === 'empty') {
    if (setting.kind === 'list') {
      return setting.items
        .map((item) => settingText(catalogue, parameter.kind, item, items, parameter))
        .join(', ');
    }
    if (setting.kind === 'pairs') return `**${text}`;
  }
  return `${parameter.name}=${text}`;
}

/**
 * Returns how each parameter of `entry` that `options` sets is written in a
 * call: only those that are given everything they need and differ from their
 * default. `items` are what a parameter that follows another is aligned with.
 */
export function optionTexts(
  catalogue: Catalogue,
  entry: EntryView | null,
  options: Options,
  items: (of: string) => readonly string[],
): string[] {
  if (entry === null) return [];
  return setParameters(entry, options)
    .map(({ parameter, setting }) => ({
      parameter,
      setting,
      items: parameter.shape.shape === 'perItem' ? items(parameter.shape.of) : [],
    }))
    .filter(
      ({ parameter, setting, items: each }) =>
        isComplete(setting) && !atDefaultOver(parameter, setting, each),
    )
    .map(({ parameter, setting, items: each }) =>
      parameterText(catalogue, parameter, setting, each),
    );
}

/**
 * Returns the Polars text of one method call: its argument, then the text of
 * each parameter that is set. A method with nothing is written with no
 * arguments.
 */
function methodCall(catalogue: Catalogue, method: Method): string {
  const settings = optionTexts(
    catalogue,
    methodEntry(catalogue, method.name),
    method.options,
    () => [],
  );
  const argument = method.argument === null ? [] : [argumentValueText(catalogue, method.argument)];
  return `.${method.name}(${[...argument, ...settings].join(', ')})`;
}
