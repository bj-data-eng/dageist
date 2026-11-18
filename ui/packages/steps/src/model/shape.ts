/**
 * @file Whether something received from another window has the shape of what
 * the editor holds: the settings of a step or method, and the values,
 * conditions and chains they are made of. Each check goes all the way down, so
 * that what passes can be drawn and worked out without looking again.
 */

import type { Options, PairSetting, Setting } from './settings';
import type { Arm, Chain, Condition, Source, Value } from './source';
import { COMPARISONS } from './source';

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

function isCell(value: unknown): boolean {
  return value === null || typeof value === 'string' || typeof value === 'number';
}

function isPairSetting(value: unknown): value is PairSetting {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    isSetting(value.key) &&
    isSetting(value.value)
  );
}

/** Reports whether `value` is one parameter's setting, and everything inside it is. */
export function isSetting(value: unknown): value is Setting {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'unset':
      return true;
    case 'flag':
      return typeof value.value === 'boolean';
    case 'number':
      return (
        typeof value.value === 'number' &&
        Number.isFinite(value.value) &&
        isMarkedFloat(value.float)
      );
    case 'text':
    case 'word':
      return typeof value.value === 'string';
    case 'expression':
      return isValue(value.value);
    case 'list':
      return everyItem(value.items, isSetting);
    case 'pairs':
      return everyItem(value.entries, isPairSetting);
    case 'each':
      return isRecord(value.items) && Object.values(value.items).every((item) => isSetting(item));
    default:
      return false;
  }
}

/** Reports whether `value` is the parameters a step or method is set with, each of them a setting. */
export function isOptions(value: unknown): value is Options {
  return isRecord(value) && Object.values(value).every((setting) => isSetting(setting));
}

/** Reports whether `value` is a value of a chain or a condition, and everything inside it is. */
export function isValue(value: unknown): value is Value {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'column':
      return typeof value.column === 'string';
    case 'literal':
      return isCell(value.value) && isMarkedFloat(value.float);
    case 'chain':
      return typeof value.id === 'string' && isChain(value.chain);
    case 'input':
      return typeof value.name === 'string';
    case 'unset':
      return true;
    default:
      return false;
  }
}

function isArm(value: unknown): value is Arm {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    isCondition(value.condition) &&
    isValue(value.returns)
  );
}

function isSource(value: unknown): value is Source {
  if (!isRecord(value)) return false;
  switch (value.kind) {
    case 'column':
      return typeof value.column === 'string';
    case 'when':
      return everyItem(value.arms, isArm) && isValue(value.otherwise);
    case 'element':
      return true;
    case 'input':
      return typeof value.name === 'string';
    default:
      return false;
  }
}

/** Reports whether `value` is a condition, and everything inside it is. */
export function isCondition(value: unknown): value is Condition {
  if (!isRecord(value) || typeof value.id !== 'string') return false;
  switch (value.kind) {
    case 'test':
      return (
        typeof value.column === 'string' &&
        COMPARISONS.some((comparison) => comparison === value.comparison) &&
        isValue(value.value)
      );
    case 'group':
      return typeof value.every === 'boolean' && everyItem(value.items, isCondition);
    case 'chain':
      return isChain(value.chain);
    default:
      return false;
  }
}

function isMethod(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    (value.argument === null || isValue(value.argument)) &&
    isOptions(value.options)
  );
}

/** Reports whether `value` is a chain, with the options of every method in it. */
export function isChain(value: unknown): value is Chain {
  return isRecord(value) && isSource(value.source) && everyItem(value.methods, isMethod);
}
