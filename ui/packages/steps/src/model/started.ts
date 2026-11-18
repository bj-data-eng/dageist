/**
 * @file Applies what the engine says a parameter holds and what a new step
 * starts with. The engine decides every one of them: Polars' default of a
 * parameter, what the parameter is set to when it is first added, and the
 * arguments of a new step on a table. What is here only reads those settings
 * as the editor's own, and builds the step the editor draws from them.
 */

import type { ParameterView, SettingView, StepStartView, StepStartsView } from '@dageist/host';

import { assertNever } from './frame';
import { nextId } from './ids';
import type { Setting } from './settings';
import { NO_OPTIONS, UNSET, expressionSetting } from './settings';
import type { Condition, Value } from './source';
import { asComparison } from './source';
import type { Options } from './settings';
import type { Step, Verb } from './steps';
import { elementChain } from './steps';

/** Raised where the engine reports a setting the editor has no place for. */
export class UnrepresentableSetting extends Error {
  constructor(what: string, held: SettingView) {
    super(`The engine gave ${what} a ${held.kind} setting, which the editor cannot hold.`);
    this.name = 'UnrepresentableSetting';
  }
}

/**
 * Returns what an expression the engine reports is, as the value an expression parameter holds.
 *
 * @throws {UnrepresentableSetting} When it is not an expression, or still to be given.
 */
function valueOf(view: SettingView, what: string): Value {
  switch (view.kind) {
    case 'unset':
      return { kind: 'unset' };
    case 'field':
      return { kind: 'column', column: view.column };
    case 'element':
      return { kind: 'chain', id: nextId('chain'), chain: elementChain(view.method) };
    default:
      throw new UnrepresentableSetting(what, view);
  }
}

/** Returns `view`, which the engine reported, as the setting of the parameter that holds it. */
export function settingOf(view: SettingView): Setting {
  switch (view.kind) {
    case 'unset':
      return UNSET;
    case 'flag':
      return { kind: 'flag', value: view.value };
    case 'integer':
      return { kind: 'number', value: view.value, shown: view.shown };
    case 'float':
      return { kind: 'number', value: view.value, float: true, shown: view.shown };
    case 'text':
      return { kind: 'text', value: view.value, shown: view.shown };
    case 'word':
      return { kind: 'word', value: view.value };
    case 'field':
    case 'element':
      return expressionSetting(valueOf(view, 'a parameter'));
    case 'comparison':
      // A comparison is a condition of a filter, which `startedStep` reads.
      throw new UnrepresentableSetting('a parameter', view);
    case 'list':
      return { kind: 'list', items: view.items.map((item) => settingOf(item)) };
    case 'pairs':
      return {
        kind: 'pairs',
        entries: view.entries.map((entry) => ({
          id: nextId('pair'),
          key: settingOf(entry.key),
          value: settingOf(entry.value),
        })),
      };
    default:
      return assertNever(view);
  }
}

/** Returns what `parameter` holds when it is not given, as a setting, or `null` when Polars decides. */
export function defaultOf(parameter: ParameterView): Setting | null {
  return parameter.defaultSetting === null ? null : settingOf(parameter.defaultSetting);
}

/** Returns what `parameter` is set to when it is first added: something that differs from its default. */
export function startOf(parameter: ParameterView): Setting {
  return settingOf(parameter.start);
}

/** The parameters of each step that its own structure holds, which are not among its options. */
const STRUCTURE: Readonly<Record<Verb, readonly string[]>> = {
  with_columns: ['named_exprs'],
  filter: ['predicates'],
  sort: [],
  select: [],
  rename: [],
  join: ['other'],
  pivot: ['on', 'index', 'values'],
};

/** Returns what the engine starts the parameter `name` of a step with, or still to be given. */
function argumentOf(start: StepStartView, name: string): SettingView {
  return (
    start.arguments.find((argument) => argument.parameter === name)?.setting ?? { kind: 'unset' }
  );
}

/**
 * Returns the name `view` holds, or none where it holds no name yet.
 *
 * @throws {UnrepresentableSetting} When it is not a name.
 */
function nameOf(view: SettingView, what: string): string {
  switch (view.kind) {
    case 'unset':
      return '';
    case 'word':
    case 'text':
      return view.value;
    default:
      throw new UnrepresentableSetting(what, view);
  }
}

/** Returns the settings of every parameter the step starts with that its structure does not hold. */
function startedOptions(verb: Verb, start: StepStartView): Options {
  const covered = STRUCTURE[verb];
  const entries = start.arguments
    .filter((argument) => !covered.includes(argument.parameter))
    .map((argument) => [argument.parameter, settingOf(argument.setting)] as const);
  return entries.length === 0 ? NO_OPTIONS : Object.fromEntries(entries);
}

/**
 * Returns the column a comparison reads, or none where it is still to be given.
 *
 * @throws {UnrepresentableSetting} When it is neither.
 */
function columnOf(view: SettingView): string {
  switch (view.kind) {
    case 'unset':
      return '';
    case 'field':
      return view.column;
    default:
      throw new UnrepresentableSetting('a filter', view);
  }
}

/**
 * Returns the condition a filter starts with, from the comparison the engine says it starts with:
 * the field it reads and what it is compared with, as the editor draws a test. The comparison is
 * the entry the engine names, which must be one the catalogue lists as a comparison; its sign is
 * the engine's to give.
 *
 * @throws {UnrepresentableSetting} When it is not a comparison.
 * @throws {Error} When the entry is not one the catalogue lists as a comparison.
 */
function startedTest(view: SettingView, comparisons: ReadonlySet<string>): Condition {
  if (view.kind !== 'comparison') throw new UnrepresentableSetting('a filter', view);
  const comparison = asComparison(view.entry, comparisons);
  if (comparison === null) {
    throw new Error(
      `The engine started a filter with ${view.entry}, which the catalogue does not list as a comparison.`,
    );
  }
  return {
    kind: 'test',
    id: nextId('test'),
    column: columnOf(view.field),
    comparison,
    value: valueOf(view.other, 'a filter'),
  };
}

/** What the engine says each step starts with, by the identifier of its entry. */
export type StepStarts = ReadonlyMap<string, StepStartView>;

/** Returns what each step starts with, by the identifier of its entry. */
export function startsByEntry(view: StepStartsView): StepStarts {
  return new Map(view.starts.map((start) => [start.entry, start]));
}

/**
 * Returns a new step of `verb`, built from what the engine says its entry starts with on the
 * table it is added to. What the engine leaves to be given is still to be picked in the step.
 * `comparisons` are the entries the catalogue lists as comparisons, which a filter may start with.
 */
export function startedStep(
  verb: Verb,
  start: StepStartView,
  comparisons: ReadonlySet<string>,
): Step {
  const id = nextId('step');
  const options = startedOptions(verb, start);
  switch (verb) {
    case 'with_columns': {
      const named = argumentOf(start, 'named_exprs');
      const first = named.kind === 'pairs' ? named.entries[0] : undefined;
      return {
        id,
        verb,
        alias: first === undefined ? '' : nameOf(first.key, 'a new column'),
        value: first === undefined ? { kind: 'unset' } : valueOf(first.value, 'a new column'),
        options,
      };
    }
    case 'filter': {
      const predicates = argumentOf(start, 'predicates');
      const first = predicates.kind === 'list' ? predicates.items[0] : undefined;
      if (first === undefined) {
        throw new Error('The engine gave a filter no condition to start with.');
      }
      return { id, verb, condition: startedTest(first, comparisons), options };
    }
    case 'sort':
    case 'select':
    case 'rename':
      return { id, verb, options };
    case 'join':
      return { id, verb, table: nameOf(argumentOf(start, 'other'), 'a join'), options };
    case 'pivot':
      return {
        id,
        verb,
        on: nameOf(argumentOf(start, 'on'), 'a pivot'),
        index: nameOf(argumentOf(start, 'index'), 'a pivot'),
        values: nameOf(argumentOf(start, 'values'), 'a pivot'),
        options,
      };
    default:
      return assertNever(verb);
  }
}
