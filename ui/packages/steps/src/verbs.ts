/**
 * @file What the steps are called, from the catalogue, and the colour each is
 * written in.
 */

import type { Catalogue } from './model/catalogue';
import { verbEntry } from './model/catalogue';
import type { Verb } from './model/steps';

/**
 * Returns what the catalogue calls the step `verb`: its head, such as "Add
 * column". A verb the catalogue lacks is called by its Polars name.
 */
export function verbHead(catalogue: Catalogue, verb: Verb): string {
  const entry = verbEntry(catalogue, verb);
  return entry?.head ?? entry?.name ?? verb;
}

/** Returns what the catalogue says the step `verb` does, or the empty text for a verb it lacks. */
export function verbNote(catalogue: Catalogue, verb: Verb): string {
  return verbEntry(catalogue, verb)?.hint ?? '';
}

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
