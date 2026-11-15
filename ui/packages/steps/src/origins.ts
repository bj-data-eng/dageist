/**
 * @file Where the fields a step works on come from. Each step's result says
 * which fields it added or replaced, so the step that last made a field can
 * be read off the results, whatever the steps are.
 */

import { createContext, useContext } from 'react';

import type { Column } from './model/frame';
import { SOURCE_NAME } from './model/frame';
import type { StepResult } from './model/steps';

/**
 * The step that last made each field, by the field's name: the step's number,
 * from 1. A field with no entry was in the table the steps start from.
 */
export type Origins = ReadonlyMap<string, number>;

/** Returns, for each step in order, where the fields it is given come from. */
export function originsBefore(results: readonly StepResult[]): readonly Origins[] {
  const each: Origins[] = [];
  const made = new Map<string, number>();
  for (const [index, result] of results.entries()) {
    each.push(new Map(made));
    for (const name of result.added) made.set(name, index + 1);
  }
  return each;
}

/** Where the fields of the step being drawn come from. */
export const OriginsContext = createContext<Origins>(new Map());

/**
 * Returns what a field's hint says: its name, its dtype and where it comes
 * from. A field that is not there has nothing to say.
 */
export function useFieldHint(): (column: Column | null) => string[] {
  const origins = useContext(OriginsContext);
  return (column) => {
    if (column === null) return [];
    const step = origins.get(column.name);
    const from = step === undefined ? `from ${SOURCE_NAME}` : `from step ${String(step)}`;
    return [`${column.name} · ${column.dtype} · ${from}`];
  };
}
