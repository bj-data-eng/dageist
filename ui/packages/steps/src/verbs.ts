/**
 * @file What the steps are called and the colour each is written in.
 */

import type { Verb } from './model/steps';

/** What each step is called: Polars' own word where it has one, in a plain phrase. */
export const VERB_LABELS: Readonly<Record<Verb, string>> = {
  with_columns: 'Add column',
  filter: 'Filter table',
  sort: 'Sort table',
  select: 'Select',
  rename: 'Rename',
  join: 'Join table',
  pivot: 'Pivot table',
};

/** What each step does, for someone choosing one. */
export const VERB_NOTES: Readonly<Record<Verb, string>> = {
  with_columns: 'a new column from a chain',
  filter: 'the rows where a condition holds',
  sort: 'the rows in the order of some fields',
  select: 'some of the columns',
  rename: 'columns under new names',
  join: 'columns from another table, by a key',
  pivot: 'the values of a field as columns',
};

/** The colour each step is written in, so the kind of a step shows at a glance. */
export const VERB_COLOURS: Readonly<Record<Verb, string>> = {
  with_columns: '[--verb:var(--color-verb-add)]',
  filter: '[--verb:var(--color-verb-filter)]',
  sort: '[--verb:var(--color-verb-sort)]',
  select: '[--verb:var(--color-verb-select)]',
  rename: '[--verb:var(--color-verb-rename)]',
  join: '[--verb:var(--color-verb-join)]',
  pivot: '[--verb:var(--color-verb-pivot)]',
};
