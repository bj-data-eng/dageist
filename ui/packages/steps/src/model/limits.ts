/**
 * @file The one list of what the step editor does not yet express of what the engine allows. Each
 * declared gap names what it covers, an entry, a place, a start, a parameter, a state of a report
 * or a kind of value typed in, and says why. Three tests find the gaps, each of the kinds that
 * `FOUND_BY` names for it, and fail on a gap that is found and not declared, and on a gap that is
 * declared and not found. The grid test runs the model's functions for every place the engine was
 * asked about; the test of states does the same for every state of a step's report; and the census
 * test holds every variant the host lists against what the editor can draw. A change of the engine
 * or of the catalogue that adds something the editor cannot draw, or a change of the editor that
 * draws it, changes this list.
 *
 * A list shows an entry that is declared here as disabled, with the reason, where it lists the
 * entry at all.
 */

import type { EntryView } from '@dageist/host';

/** The kinds of what an expression starts from, for a gap in what can follow a start. */
export type StartKind =
  /** A field of the rows. */
  | 'field'
  /** A connected single value. */
  | 'connectedValue'
  /** A connected list. */
  | 'connectedList'
  /** A whole number typed in. */
  | 'typedInteger'
  /** A number with a fraction typed in. */
  | 'typedFloat'
  /** Text typed in. */
  | 'typedText'
  /** A flag, which a document may hold typed in. */
  | 'typedFlag'
  /** A function that starts a chain. */
  | 'function';

/** The states of a step's report that the editor has no words for. */
export type ReportState =
  /** A step Polars accepts that was not counted because a step before it failed on the rows. */
  'notCountedAfterRowsFailure';

/** What a declared gap covers. */
export type Cover =
  /** An entry the engine allows that the editor cannot pick or make. */
  | { readonly kind: 'entry'; readonly entry: string }
  /** A place that no word of the editor stands at: where the outer position or the nested one is not drawn. */
  | { readonly kind: 'place'; readonly outer: string; readonly nested: string | null }
  /** What an expression starts from, after which the editor cannot add a method that the engine allows. */
  | { readonly kind: 'start'; readonly start: StartKind }
  /** A parameter of an entry that its drawing does not set. */
  | { readonly kind: 'parameter'; readonly entry: string; readonly parameter: string }
  /** A kind of parameter that has no drawing, by the shape and kind that make it up. */
  | { readonly kind: 'parameterKind'; readonly of: string }
  /** A state of a step's report that says nothing. */
  | { readonly kind: 'report'; readonly state: ReportState }
  /** A kind of number, text or flag a document holds typed in, which the editor has no way to type. */
  | { readonly kind: 'typed'; readonly literal: string };

/** What finds the gaps of each kind of cover: the grid test, the test of states, or the census test. */
export const FOUND_BY = {
  entry: 'grid',
  place: 'grid',
  start: 'grid',
  parameter: 'grid',
  parameterKind: 'grid',
  report: 'states',
  typed: 'census',
} as const satisfies Readonly<Record<Cover['kind'], 'grid' | 'states' | 'census'>>;

/** One declared gap: what it covers and why. */
export interface Gap {
  readonly cover: Cover;
  readonly reason: string;
}

/** Returns what tells one cover from another, so that a gap found can be matched with one declared. */
export function coverKey(cover: Cover): string {
  switch (cover.kind) {
    case 'entry':
      return `entry ${cover.entry}`;
    case 'place':
      return `place ${cover.outer} ${cover.nested ?? 'alone'}`;
    case 'start':
      return `start ${cover.start}`;
    case 'parameter':
      return `parameter ${cover.entry} ${cover.parameter}`;
    case 'parameterKind':
      return `parameterKind ${cover.of}`;
    case 'report':
      return `report ${cover.state}`;
    case 'typed':
      return `typed ${cover.literal}`;
    default:
      return cover satisfies never;
  }
}

/** Why a further case of a condition is not drawn. */
const FURTHER_CASE = 'the editor draws one case of a when; a further case follows otherwise';

/** Why a function that starts a chain is not offered. */
const NOT_A_START =
  'the editor starts a chain from a field, an input, a typed value, when and element; this function is not offered where an expression starts';

/** Why nothing is added after a connected input or a typed value. */
const NOTHING_AFTER_A_WORD =
  'a connected input or a typed value is drawn as one word that offers no method after it, though the engine allows methods there (#171)';

/** Why no flag can be typed in. */
const NO_TYPED_FLAG =
  'a document may hold a flag typed in as the value of lit, which the engine allows, but the host reads typed text as a number or as text and never as a flag, so the editor has no way to type one; a flag a document holds is drawn as a function';

/** Declares the entry `entry`, which the editor cannot pick or make, for `reason`. */
function entryGap(entry: string, reason: string): Gap {
  return { cover: { kind: 'entry', entry }, reason };
}

/** Declares the parameter `parameter` of `entry`, which its drawing does not set, for `reason`. */
function parameterGap(entry: string, parameter: string, reason: string): Gap {
  return { cover: { kind: 'parameter', entry, parameter }, reason };
}

/** Declares that nothing is added after what an expression starts from as `start`. */
function startGap(start: StartKind): Gap {
  return { cover: { kind: 'start', start }, reason: NOTHING_AFTER_A_WORD };
}

/** Every gap the editor has, declared. */
export const GAPS: readonly Gap[] = [
  // The one case of a condition the editor draws ends with otherwise; a further case is not drawn.
  entryGap('ChainedThen.otherwise', FURTHER_CASE),
  entryGap('ChainedThen.when', FURTHER_CASE),
  entryGap('ChainedWhen.then', FURTHER_CASE),
  entryGap('Then.when', FURTHER_CASE),
  // The functions the editor does not start a chain from.
  entryGap('polars.date', NOT_A_START),
  entryGap('polars.date_range', NOT_A_START),
  entryGap('polars.datetime', NOT_A_START),
  entryGap('polars.datetime_range', NOT_A_START),
  entryGap('polars.duration', NOT_A_START),
  entryGap('polars.from_epoch', NOT_A_START),
  entryGap('polars.int_range', NOT_A_START),
  entryGap('polars.linear_space', NOT_A_START),
  entryGap('polars.repeat', NOT_A_START),
  entryGap('polars.time', NOT_A_START),
  entryGap('polars.time_range', NOT_A_START),
  // The parameters of the entries the editor draws as a word of their own, which it does not set.
  parameterGap(
    'polars.col',
    'more_names',
    'a field is picked by its name; several names at once are not drawn',
  ),
  parameterGap(
    'polars.lit',
    'dtype',
    'a number or text is typed in; the dtype of a literal is not drawn',
  ),
  parameterGap(
    'polars.lit',
    'allow_object',
    'a number or text is typed in; allowing an object is not drawn',
  ),
  parameterGap(
    'polars.when',
    'constraints',
    'a condition is its predicates; keyword constraints are not drawn',
  ),
  // What an expression starts from, after which the editor offers no method.
  startGap('connectedValue'),
  startGap('connectedList'),
  startGap('typedInteger'),
  startGap('typedFloat'),
  startGap('typedText'),
  { cover: { kind: 'typed', literal: 'flag' }, reason: NO_TYPED_FLAG },
  {
    cover: { kind: 'report', state: 'notCountedAfterRowsFailure' },
    reason:
      'a step after one that failed on the rows is accepted and not counted, so the editor says nothing of it (#171)',
  },
];

/**
 * Returns why the editor cannot pick or make `entry` where the engine allows it, or `null` when a
 * list may offer it. A list that has a row for such an entry shows it disabled with this reason.
 */
export function editorLimit(entry: EntryView): string | null {
  return (
    GAPS.find((gap) => gap.cover.kind === 'entry' && gap.cover.entry === entry.id)?.reason ?? null
  );
}
