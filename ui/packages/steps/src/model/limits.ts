/**
 * @file What the editor cannot draw or express of what the engine allows. The
 * engine decides which calls may stand where; the editor draws a chain as a
 * start and methods, and a condition as one `when`, one `then` and one
 * `otherwise`. What the editor has no word for is written here, once, so that
 * a list shows it disabled with the reason and the check of what the editor
 * cannot offer lists it from the engine's own offers.
 */

import type { AfterView, EntryView, OfferPlaceView } from '@dageist/host';

import type { Catalogue } from './catalogue';

/** The links of a condition that the editor draws: one case, which `otherwise` ends. */
const DRAWN_CONDITION_LINKS: ReadonlySet<string> = new Set([
  'polars.when',
  'When.then',
  'Then.otherwise',
]);

/** Why a further case of a condition is not drawn. */
const FURTHER_CASE = 'the editor draws one case of a when; a further case follows otherwise';

/**
 * Returns why the editor cannot draw `entry` wherever the engine allows it, or
 * `null` when it can. It judges only what the editor can draw, not whether the
 * engine allows the entry at a place.
 */
export function editorLimit(entry: EntryView): string | null {
  if (entry.applies === 'condition' && !DRAWN_CONDITION_LINKS.has(entry.id)) return FURTHER_CASE;
  return null;
}

/**
 * The parameters the editor sets of an entry whose parameters it does not all
 * set. An entry that is not here has every parameter set: each kind of
 * parameter has a drawing.
 */
const SET_BY_THE_EDITOR: Readonly<Record<string, readonly string[]>> = {
  // A field is picked by its name; several names at once are not drawn.
  'polars.col': ['name'],
  // A number or text is typed in; a literal of a given dtype is not drawn.
  'polars.lit': ['value'],
  // A condition is its predicates; keyword constraints are not drawn.
  'polars.when': ['predicates'],
  'Then.when': ['predicates'],
  'ChainedThen.when': ['predicates'],
};

/** Returns the parameters of `entry` that the editor has no way to set. */
export function unexpressedParameters(entry: EntryView): readonly string[] {
  const set = Object.hasOwn(SET_BY_THE_EDITOR, entry.id) ? SET_BY_THE_EDITOR[entry.id] : undefined;
  if (set === undefined) return [];
  return entry.parameters.map((parameter) => parameter.name).filter((name) => !set.includes(name));
}

/** The place of one cell of the grid of every place the engine was asked about, and which entries may stand there. */
export interface Cell {
  readonly place: OfferPlaceView;
  readonly allowed: readonly string[];
}

/** What the engine allows at some places that the editor cannot draw. */
export interface Unofferable {
  readonly entry: string;
  /** What an expression has where the entry may stand, in words: nothing, a value, or a condition at a state. */
  readonly after: string;
  /** In how many places of that kind the engine allows it. */
  readonly places: number;
  /** What the editor cannot do. */
  readonly reason: string;
}

/** Returns what an expression has, in words, without its dtype or shape, which no limit of the editor depends on. */
function afterWords(after: AfterView): string {
  switch (after.kind) {
    case 'start':
      return 'start';
    case 'value':
      return 'value';
    case 'condition':
      return `condition at ${after.state}`;
    default:
      return after satisfies never;
  }
}

/**
 * Returns every combination the engine allows in `cells` that the editor
 * cannot draw, grouped by the entry, what an expression has there and why.
 * An entry the editor does not know is one it cannot draw.
 */
export function unofferable(catalogue: Catalogue, cells: readonly Cell[]): Unofferable[] {
  const found = new Map<string, Unofferable>();
  for (const cell of cells) {
    for (const id of cell.allowed) {
      const entry = catalogue.entries.get(id);
      const reason = entry === undefined ? 'the catalogue has no such entry' : editorLimit(entry);
      if (reason === null) continue;
      const after = afterWords(cell.place.after);
      const key = `${id}\u0000${after}\u0000${reason}`;
      const held = found.get(key);
      found.set(key, { entry: id, after, places: (held?.places ?? 0) + 1, reason });
    }
  }
  const listed = [...found.values()];
  listed.sort(
    (left, right) => left.entry.localeCompare(right.entry) || left.after.localeCompare(right.after),
  );
  return listed;
}
