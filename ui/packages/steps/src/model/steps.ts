/**
 * @file The step list in Polars' own vocabulary: `with_columns`, `filter`,
 * `sort`, `select`, `rename`, `join` and `pivot`, as the editor holds them.
 * What a step does to the table, and whether it may, is the engine's to say;
 * `document.ts` writes the steps as the document the engine runs.
 */

import type { ParameterView } from '@dageist/host';

import type { MethodName } from './expr';
import { verbEntry } from './catalogue';
import type { Catalogue } from './catalogue';
import type { Frame } from './frame';
import { assertNever, quoted } from './frame';
import { nextId } from './ids';
import type { NamedTable } from './inputs';
import type { Options, Setting } from './settings';
import {
  NO_OPTIONS,
  UNSET,
  expressionSetting,
  itemsOf,
  optionOf,
  optionParameters,
  withoutOptions,
} from './settings';
import type { Chain, Condition, Value } from './source';
import { conditionText, keyText, optionTexts, settingValueText, valueExprText } from './source';

/** The step verbs, by their Polars names. */
export type Verb = 'with_columns' | 'filter' | 'sort' | 'select' | 'rename' | 'join' | 'pivot';

/** Every verb, in the order it is offered. */
export const VERBS: readonly Verb[] = [
  'with_columns',
  'filter',
  'sort',
  'select',
  'rename',
  'join',
  'pivot',
];

/** `.with_columns(value.alias(alias))`: adds a column, or replaces one of the same name. */
export interface WithColumnsStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'with_columns';
  /** The name of the column. */
  readonly alias: string;
  /** What the column holds: a field, a value written out, an input, or a chain. */
  readonly value: Value;
  /** The other parameters of `with_columns` that are set. */
  readonly options: Options;
}

/**
 * `.sort(by…, descending=[…], nulls_last=[…])`: orders the rows by one key
 * after another. The keys are the list `by` of `options`, each an expression.
 * Which way each is ordered, and where the empty values go, are parameters
 * that are set for each key in `options` too.
 */
export interface SortStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'sort';
  readonly options: Options;
}

/**
 * `.select(exprs…)`: keeps some of the columns, in the order given. The
 * expressions are the list `exprs` of `options`.
 */
export interface SelectStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'select';
  readonly options: Options;
}

/**
 * `.rename({from: to, …})`: gives columns new names. The pairs are the
 * parameter `mapping` of `options`.
 */
export interface RenameStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'rename';
  readonly options: Options;
}

/** `.filter(condition)`: keeps the rows where the condition is true. */
export interface FilterStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'filter';
  readonly condition: Condition;
  readonly options: Options;
}

/**
 * `.join(table, left_on=…, right_on=…, how=…)`: brings in another table's
 * columns by their keys. The keys are the lists `left_on` and `right_on` of
 * `options`, each key an expression, matched with the one at its place on the
 * other side. Which rows it keeps, `how`, and the other parameters that are
 * set are in `options` too.
 */
export interface JoinStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'join';
  /** The name of the other table. */
  readonly table: string;
  readonly options: Options;
}

/**
 * `.pivot(on, on_columns=…, index=…, values=…, aggregate_function=…)`: turns
 * the values of one column into columns. The columns it returns are the
 * values of a connected table, the parameter `on_columns` of `options`. How
 * the values that land in one cell are combined is the expression
 * `aggregate_function` of `options`, made of `pl.element()`.
 */
export interface PivotStep {
  /** Stays with the step while it is moved or edited. */
  readonly id: string;
  readonly verb: 'pivot';
  readonly on: string;
  readonly index: string;
  readonly values: string;
  readonly options: Options;
}

/** One step of the list. */
export type Step =
  | WithColumnsStep
  | FilterStep
  | SortStep
  | SelectStep
  | RenameStep
  | JoinStep
  | PivotStep;

/** A value that is still to be picked. */
const NOTHING: Value = { kind: 'unset' };

/** Returns the keys of the list parameter `name` of `options`: each item's expression, or nothing yet. */
export function keysOf(options: Options, name: string): readonly Value[] {
  return itemsOf(options, name).map((item) => (item.kind === 'expression' ? item.value : NOTHING));
}

/** Returns a list of keys that are the fields called `names`, as the parameter of a step holds them. */
export function fieldKeys(names: readonly string[]): Setting {
  return {
    kind: 'list',
    items: names.map((column) => expressionSetting({ kind: 'column', column })),
  };
}

/** Returns what `pl.element()` followed by the method `name` gives: the chain an aggregate is made of. */
export function elementChain(name: MethodName): Chain {
  return {
    source: { kind: 'element' },
    methods: [{ id: nextId('method'), name, argument: null, options: NO_OPTIONS }],
  };
}

/** Returns the aggregate of `step`: the expression its values are combined by, when it holds one. */
export function aggregateOf(step: PivotStep): Value | null {
  const setting = optionOf(step.options, 'aggregate_function');
  return setting?.kind === 'expression' ? setting.value : null;
}

/** Returns how `key` is told apart from the other keys of a list: a field by its name, anything else by its text. */
function keyLabel(catalogue: Catalogue, key: Value): string {
  return key.kind === 'column' ? key.column : keyText(catalogue, key);
}

/** The parameters each step's own structure draws, which are not offered as options. */
const COVERED: Readonly<Record<Verb, readonly string[]>> = {
  // A column and what it holds are the step's own structure.
  with_columns: ['exprs', 'named_exprs'],
  filter: ['predicates'],
  // The fields to order by are one list, so a second one for more fields is not offered.
  sort: ['by', 'more_by'],
  // The fields to keep are one list, so another expression is not offered by name.
  select: ['exprs'],
  rename: ['mapping'],
  // The keys of the two tables are one list each, so `on` for a shared key is not offered.
  join: ['other', 'on', 'left_on', 'right_on'],
  pivot: ['on', 'index', 'values', 'aggregate_function'],
};

/** Returns the parameters of the step `verb` that can be added to it as options. */
export function stepOptionParameters(catalogue: Catalogue, verb: Verb): readonly ParameterView[] {
  const entry = verbEntry(catalogue, verb);
  return entry === null ? [] : optionParameters(entry, COVERED[verb]);
}

/**
 * Returns the items a parameter of `step` that follows the parameter `of`
 * holds one value for each of: the keys of a sort that are given, each by the
 * name of its field or else its text.
 */
export function stepItems(catalogue: Catalogue, step: Step, of: string): readonly string[] {
  if (step.verb !== 'sort' || of !== 'by') return [];
  return keysOf(step.options, 'by').flatMap((key) =>
    key.kind === 'unset' ? [] : [keyLabel(catalogue, key)],
  );
}

/**
 * Returns a new step of `verb` that works on `frame` as it stands, with the
 * connected `tables` a join or a pivot can use.
 */
export function defaultStep(verb: Verb, frame: Frame, tables: readonly NamedTable[]): Step {
  const id = nextId('step');
  const options = NO_OPTIONS;
  switch (verb) {
    case 'with_columns':
      return {
        id,
        verb,
        alias: 'new_column',
        // What the column holds is still to be picked.
        value: { kind: 'unset' },
        options,
      };
    case 'sort':
      // The first key is still to be picked.
      return { id, verb, options: { by: { kind: 'list', items: [UNSET] } } };
    case 'select':
      return {
        id,
        verb,
        options: { exprs: fieldKeys(frame.map((column) => column.name)) },
      };
    case 'rename':
      return { id, verb, options: { mapping: { kind: 'pairs', entries: [] } } };
    case 'filter':
      return {
        id,
        verb,
        condition: {
          // A comparison whose field and value are still to be picked.
          kind: 'test',
          id: nextId('test'),
          column: '',
          comparison: '==',
          value: { kind: 'unset' },
        },
        options,
      };
    case 'join': {
      // Start from the first lookup table. Which fields are its keys is still to be picked: the
      // engine says whether the two sides fit, and it is not guessed here.
      const [table] = tables;
      return {
        id,
        verb,
        table: table?.name ?? '',
        options: {
          left_on: { kind: 'list', items: [UNSET] },
          right_on: { kind: 'list', items: [UNSET] },
          // Rows with no match are kept, which is not what Polars does unless it is asked.
          how: { kind: 'word', value: 'left' },
        },
      };
    }
    case 'pivot': {
      // The three fields are still to be picked, and the columns come from the first connected
      // table, which the step is then judged with.
      const [columns] = tables;
      return {
        id,
        verb,
        on: '',
        index: '',
        values: '',
        options: {
          on_columns: columns === undefined ? UNSET : { kind: 'word', value: columns.name },
          aggregate_function: expressionSetting({
            kind: 'chain',
            id: nextId('chain'),
            chain: elementChain('first'),
          }),
        },
      };
    }
    default:
      return assertNever(verb);
  }
}

/** Returns `args` as the arguments of a call, then the text of the parameters that are set. */
function callText(args: readonly string[], settings: readonly string[]): string {
  return [...args, ...settings].join(', ');
}

/** Returns the Polars text of a pivot. */
function pivotText(catalogue: Catalogue, step: PivotStep, settings: readonly string[]): string {
  const aggregate = optionOf(step.options, 'aggregate_function');
  const entry = verbEntry(catalogue, step.verb);
  const parameter = entry?.parameters.find((candidate) => candidate.name === 'aggregate_function');
  const given = [
    `index=${quoted(step.index)}`,
    `values=${quoted(step.values)}`,
    ...(aggregate === null || parameter === undefined
      ? []
      : [`aggregate_function=${settingValueText(catalogue, parameter, aggregate, [])}`]),
  ];
  return `.pivot(${quoted(step.on)}, ${callText(given, settings)})`;
}

/** Returns `texts` as the argument of a call: the one, or a list of them. */
function listText(texts: readonly string[]): string {
  const [only] = texts;
  return texts.length === 1 && only !== undefined ? only : `[${texts.join(', ')}]`;
}

/** Returns the Polars method call `step` stands for. */
export function stepText(catalogue: Catalogue, step: Step): string {
  const entry = verbEntry(catalogue, step.verb);
  // Only the parameters that are set and differ from their default are written,
  // and not those the step writes itself.
  const settings = optionTexts(
    catalogue,
    entry,
    withoutOptions(step.options, COVERED[step.verb]),
    (of) => stepItems(catalogue, step, of),
  );
  switch (step.verb) {
    case 'with_columns':
      return `.with_columns(${callText([`${valueExprText(catalogue, step.value)}.alias(${quoted(step.alias)})`], settings)})`;
    case 'sort':
      return `.sort(${callText(
        keysOf(step.options, 'by').map((key) => keyText(catalogue, key)),
        settings,
      )})`;
    case 'select':
      return `.select(${callText(
        keysOf(step.options, 'exprs').map((key) => keyText(catalogue, key)),
        settings,
      )})`;
    case 'rename': {
      const mapping = entry?.parameters.find((candidate) => candidate.name === 'mapping');
      const setting = optionOf(step.options, 'mapping');
      const text =
        mapping === undefined || setting === null
          ? '{}'
          : settingValueText(catalogue, mapping, setting, []);
      return `.rename(${callText([text], settings)})`;
    }
    case 'filter':
      return `.filter(${callText([conditionText(catalogue, step.condition)], settings)})`;
    case 'join': {
      const keys = (name: string): string =>
        listText(keysOf(step.options, name).map((key) => keyText(catalogue, key)));
      return `.join(${callText([step.table, `left_on=${keys('left_on')}`, `right_on=${keys('right_on')}`], settings)})`;
    }
    case 'pivot':
      return pivotText(catalogue, step, settings);
    default:
      return assertNever(step);
  }
}
