/**
 * @file What the steps are called, from the catalogue, and the colour each is
 * written in.
 */

import type { Catalogue, Verb } from './model/catalogue';
import { verbOf } from './model/catalogue';

/**
 * Returns what the catalogue calls the step of the entry `entry`: its head, such as "Add column". An
 * entry the catalogue lacks is called by its identifier.
 */
export function stepHead(catalogue: Catalogue, entry: string): string {
  const found = catalogue.entries.get(entry);
  return found?.head ?? found?.name ?? entry;
}

/** Returns what the catalogue says the step of `entry` does, or the empty text for an entry it lacks. */
export function stepNote(catalogue: Catalogue, entry: string): string {
  return catalogue.entries.get(entry)?.hint ?? '';
}

/** The colour each step is written in, so the kind of a step shows at a glance. */
const VERB_COLOURS: Readonly<Record<Verb, string>> = {
  with_columns: '[--verb:var(--color-verb-add)]',
  filter: '[--verb:var(--color-verb-filter)]',
  sort: '[--verb:var(--color-verb-sort)]',
  group_by: '[--verb:var(--color-verb-group)]',
  select: '[--verb:var(--color-verb-select)]',
  rename: '[--verb:var(--color-verb-rename)]',
  join: '[--verb:var(--color-verb-join)]',
  pivot: '[--verb:var(--color-verb-pivot)]',
};

/** Returns the colour of the step of `entry`: its verb's, or the plain one for a step with no verb. */
export function stepColour(entry: string): string {
  const verb = verbOf(entry);
  return verb === null ? '[--verb:var(--color-ink)]' : VERB_COLOURS[verb];
}
