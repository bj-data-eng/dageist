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
import type { Options } from './settings';
import type { Step, Verb } from './steps';
import { elementChain } from './steps';

/** Returns what an expression the engine reports is, as the value an expression parameter holds. */
function valueOf(view: SettingView): Value {
  switch (view.kind) {
    case 'field':
      return { kind: 'column', column: view.column };
    case 'element':
      return { kind: 'chain', id: nextId('chain'), chain: elementChain(view.method) };
    default:
      // Nothing else is an expression: it is still to be picked.
      return { kind: 'unset' };
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
      return { kind: 'number', value: view.value };
    case 'float':
      return { kind: 'number', value: view.value, float: true };
    case 'text':
      return { kind: 'text', value: view.value };
    case 'word':
      return { kind: 'word', value: view.value };
    case 'field':
    case 'element':
      return expressionSetting(valueOf(view));
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

/** Returns the name `view` holds, or none where it holds no name yet. */
function nameOf(view: SettingView): string {
  return view.kind === 'word' || view.kind === 'text' ? view.value : '';
}

/** Returns the settings of every parameter the step starts with that its structure does not hold. */
function startedOptions(verb: Verb, start: StepStartView): Options {
  const covered = STRUCTURE[verb];
  const entries = start.arguments
    .filter((argument) => !covered.includes(argument.parameter))
    .map((argument) => [argument.parameter, settingOf(argument.setting)] as const);
  return entries.length === 0 ? NO_OPTIONS : Object.fromEntries(entries);
}

/** Returns the comparison a filter starts with, whose field and value are still to be picked. */
function startedTest(): Condition {
  return {
    kind: 'test',
    id: nextId('test'),
    column: '',
    comparison: '==',
    value: { kind: 'unset' },
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
 */
export function startedStep(verb: Verb, start: StepStartView): Step {
  const id = nextId('step');
  const options = startedOptions(verb, start);
  switch (verb) {
    case 'with_columns': {
      const named = argumentOf(start, 'named_exprs');
      const first = named.kind === 'pairs' ? named.entries[0] : undefined;
      return {
        id,
        verb,
        alias: first === undefined ? '' : nameOf(first.key),
        value: first === undefined ? { kind: 'unset' } : valueOf(first.value),
        options,
      };
    }
    case 'filter':
      return { id, verb, condition: startedTest(), options };
    case 'sort':
    case 'select':
    case 'rename':
      return { id, verb, options };
    case 'join':
      return { id, verb, table: nameOf(argumentOf(start, 'other')), options };
    case 'pivot':
      return {
        id,
        verb,
        on: nameOf(argumentOf(start, 'on')),
        index: nameOf(argumentOf(start, 'index')),
        values: nameOf(argumentOf(start, 'values')),
        options,
      };
    default:
      return assertNever(verb);
  }
}
