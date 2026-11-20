/**
 * @file Whether something received from another window has the shape of what
 * the editor holds: the settings of a step or method, and the values,
 * conditions and chains they are made of. Each check goes all the way down, so
 * that what passes can be drawn and worked out without looking again.
 */

import type { Options, PairSetting, Setting } from './settings';
import type { Arm, Chain, Condition, Source, Value } from './source';
import { asComparison } from './source';

/** The entries the catalogue lists as comparisons, which a test may hold, besides membership in a list. */
export type Comparisons = ReadonlySet<string>;

/** Reports whether `value` is an object with named parts: not null, and not a list. */
export function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Reports whether every item of `items` is a list of things that satisfy `holds`. */
function everyItem(items: unknown, holds: (item: unknown) => boolean): boolean {
  return Array.isArray(items) && items.every((item: unknown) => holds(item));
}

/** Reports whether `value` is what a number carries to say it was read as one with a fraction: nothing, or `true`. */
function isMarkedFloat(value: unknown): boolean {
  return value === undefined || value === true;
}

/** Reports whether `value` is what a number or text holds: a number or text, with the words the engine shows it as. */
function isTyped(value: Readonly<Record<string, unknown>>): boolean {
  return (
    (typeof value.value === 'string' || typeof value.value === 'number') &&
    typeof value.shown === 'string' &&
    isMarkedFloat(value.float)
  );
}

function isPairSetting(value: unknown, comparisons: Comparisons): value is PairSetting {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    isSetting(value.key, comparisons) &&
    isSetting(value.value, comparisons)
  );
}

/** Reports whether `value` is one parameter's setting, and everything inside it is. */
export function isSetting(value: unknown, comparisons: Comparisons): value is Setting {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'unset':
      return true;
    case 'flag':
      return typeof value.value === 'boolean';
    case 'number':
      return typeof value.value === 'number' && Number.isFinite(value.value) && isTyped(value);
    case 'text':
      return typeof value.value === 'string' && isTyped(value);
    case 'word':
      return typeof value.value === 'string';
    case 'expression':
      return isValue(value.value, comparisons);
    case 'list':
      return everyItem(value.items, (item) => isSetting(item, comparisons));
    case 'pairs':
      return everyItem(value.entries, (entry) => isPairSetting(entry, comparisons));
    case 'each':
      return (
        isRecord(value.items) &&
        Object.values(value.items).every((item) => isSetting(item, comparisons))
      );
    default:
      return false;
  }
}

/** Reports whether `value` is the parameters a step or method is set with, each of them a setting. */
export function isOptions(value: unknown, comparisons: Comparisons): value is Options {
  return (
    isRecord(value) && Object.values(value).every((setting) => isSetting(setting, comparisons))
  );
}

/** Reports whether `value` is a value of a chain or a condition, and everything inside it is. */
export function isValue(value: unknown, comparisons: Comparisons): value is Value {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'column':
      return typeof value.column === 'string';
    case 'literal':
      return value.value === null || isTyped(value);
    case 'chain':
      return typeof value.id === 'string' && isChain(value.chain, comparisons);
    case 'input':
      return typeof value.name === 'string';
    case 'unset':
    case 'leftOut':
      return true;
    default:
      return false;
  }
}

function isArm(value: unknown, comparisons: Comparisons): value is Arm {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    isCondition(value.condition, comparisons) &&
    isValue(value.returns, comparisons)
  );
}

function isSource(value: unknown, comparisons: Comparisons): value is Source {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'column':
      return typeof value.column === 'string';
    case 'when':
      return (
        everyItem(value.arms, (arm) => isArm(arm, comparisons)) &&
        isValue(value.otherwise, comparisons)
      );
    case 'element':
      return true;
    case 'input':
      return typeof value.name === 'string';
    default:
      return false;
  }
}

/** Reports whether `value` is a condition, and everything inside it is. */
export function isCondition(value: unknown, comparisons: Comparisons): value is Condition {
  if (!isRecord(value) || typeof value.id !== 'string') return false;
  switch (value.kind) {
    case 'test':
      return (
        typeof value.column === 'string' &&
        typeof value.comparison === 'string' &&
        asComparison(value.comparison, comparisons) !== null &&
        isValue(value.value, comparisons)
      );
    case 'group':
      return (
        typeof value.every === 'boolean' &&
        everyItem(value.items, (item) => isCondition(item, comparisons))
      );
    case 'chain':
      return isChain(value.chain, comparisons);
    default:
      return false;
  }
}

function isMethod(value: unknown, comparisons: Comparisons): boolean {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    (value.argument === null || isValue(value.argument, comparisons)) &&
    isOptions(value.options, comparisons)
  );
}

/** Reports whether `value` is a chain, with the options of every method in it. */
export function isChain(value: unknown, comparisons: Comparisons): value is Chain {
  return (
    isRecord(value) &&
    isSource(value.source, comparisons) &&
    everyItem(value.methods, (method) => isMethod(method, comparisons))
  );
}
