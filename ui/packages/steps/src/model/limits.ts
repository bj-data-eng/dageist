/**
 * @file The one list of what the step editor does not yet express of what the engine allows. Each
 * declared gap names what it covers, an entry, a place, a start, a parameter, a step, a parameter of
 * a step, an expression the walk gives no context, a kind of value typed in, or something a
 * document or a report holds that the editor does not show, and says why. Three tests find the
 * gaps, each of the kinds that `FOUND_BY` names for it, and fail on a gap that is found and not
 * declared, and on a gap that is declared and not found. The grid test runs the model's functions
 * for every place the engine was asked about; the census
 * test holds every variant the host lists against what the editor can draw; and the corpus test
 * rebuilds every piece of every document the engine made, and finds the steps, the parameters of
 * steps and the expressions without a context that the grid does not reach. The corpus test accepts any gap declared of a kind another test
 * finds, for a piece it cannot build: it only names the piece. A change of the engine or of the
 * catalogue that adds something the editor cannot draw, or a change of the editor that draws it,
 * changes this list.
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
  /** An expression of a document that the engine's walk walks and gives no context, so no list can be asked what may stand at it. */
  | { readonly kind: 'noContext'; readonly entry: string; readonly parameter: string }
  /** A step of the catalogue that no list adds, though a document may hold it. */
  | { readonly kind: 'step'; readonly entry: string }
  /** A parameter of a step that the editor draws neither in the step's head nor as an option. */
  | { readonly kind: 'stepParameter'; readonly entry: string; readonly parameter: string }
  /** A kind of number, text or flag a document holds typed in, which the editor has no way to type. */
  | { readonly kind: 'typed'; readonly literal: string }
  /** What a document or a report holds that the editor does not show, by what it is. */
  | { readonly kind: 'shown'; readonly what: string };

/**
 * What finds the gaps of each kind of cover: the grid test, the census test, or
 * the corpus test.
 */
export const FOUND_BY = {
  entry: 'grid',
  place: 'grid',
  start: 'grid',
  parameter: 'grid',
  parameterKind: 'grid',
  noContext: 'corpus',
  step: 'corpus',
  stepParameter: 'corpus',
  typed: 'census',
  shown: 'census',
} as const satisfies Readonly<Record<Cover['kind'], 'grid' | 'census' | 'corpus'>>;

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
    case 'noContext':
      return `noContext ${cover.entry} ${cover.parameter}`;
    case 'step':
      return `step ${cover.entry}`;
    case 'stepParameter':
      return `stepParameter ${cover.entry} ${cover.parameter}`;
    case 'typed':
      return `typed ${cover.literal}`;
    case 'shown':
      return `shown ${cover.what}`;
    default:
      return cover satisfies never;
  }
}

/** Why a further case of a condition is not drawn. */
const FURTHER_CASE = 'the editor draws one case of a when; a further case follows otherwise';

/** Why a function that starts a chain is not offered. */
const NOT_A_START =
  'the editor starts a chain from a field, an input, a typed value, when and element; this function is not offered where an expression starts';

/** Why no flag can be typed in. */
const NO_TYPED_FLAG =
  'a document may hold a flag typed in as the value of lit, which the engine allows, but the host reads typed text as a number or as text and never as a flag, so the editor has no way to type one (#177)';

/** Why a flag a document holds is not shown. */
const FLAG_NOT_SHOWN =
  'a flag a document holds typed in as the value of lit is drawn as a function whose value is the literal control, which shows a number or text and no flag, so the value is drawn as its empty label, marked wrong (#177)';

/** Why the dtypes of an expression that gives several columns are not shown. */
const SEVERAL_NOT_SHOWN =
  'the editor shows the dtype after a chain only where it gives one column; the dtypes of a chain that gives several are not shown (#177)';

/** Why the name of a field of a struct is offered only where the walk reports the fields. */
const FIELDS_LOST =
  'the editor offers the names of the fields the walk reports for the struct the method follows; where the walk lost them, as after a condition of structs or in the values of a pivot, none is offered and the engine refuses a name that is no field (#185)';

/** Why a step the editor draws by its parameters alone is not added from the list of steps. */
const NOT_A_STEP_OF_THE_LIST =
  'the list of steps holds the seven steps the editor has heads for; this step is drawn by its parameters alone, so a document may hold it and no list adds it (#137)';

/** Why a step that groups, and the calls after `group_by`, are not drawn. */
const GROUPING_NOT_DRAWN =
  'the engine holds a step that groups as `group_by` and the calls on the group by that follow it, and the editor does not draw it yet (#114)';

/** Declares the place `outer` that holds an expression of a step that groups, alone and inside each of the four nested positions, which no word of the editor stands at. */
function groupingPlaces(outer: string): Gap[] {
  return [null, 'over.partition_by', 'over.order_by', 'condition.predicate', 'condition.value'].map(
    (nested) => ({ cover: { kind: 'place', outer, nested }, reason: GROUPING_NOT_DRAWN }),
  );
}

/** Declares the step `entry`, which no list adds. */
function stepGap(entry: string): Gap {
  return { cover: { kind: 'step', entry }, reason: NOT_A_STEP_OF_THE_LIST };
}

/** Declares the entry `entry`, which the editor cannot pick or make, for `reason`. */
function entryGap(entry: string, reason: string): Gap {
  return { cover: { kind: 'entry', entry }, reason };
}

/** Declares the parameter `parameter` of `entry`, which its drawing does not set, for `reason`. */
function parameterGap(entry: string, parameter: string, reason: string): Gap {
  return { cover: { kind: 'parameter', entry, parameter }, reason };
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
  entryGap('polars.struct', NOT_A_START),
  entryGap('polars.time', NOT_A_START),
  entryGap('polars.time_range', NOT_A_START),
  // The names of a struct's fields, which are offered where the walk reports them.
  parameterGap('Expr.struct.drop', 'names', FIELDS_LOST),
  parameterGap('Expr.struct.field', 'name', FIELDS_LOST),
  // What a step that groups holds, which the editor does not draw: where its keys, its aggregates
  // and its having stand, and the step itself.
  ...groupingPlaces('group_key'),
  ...groupingPlaces('aggregate'),
  ...groupingPlaces('having'),
  { cover: { kind: 'step', entry: 'LazyFrame.group_by' }, reason: GROUPING_NOT_DRAWN },
  // The steps no list adds: the editor adds the seven it has heads for.
  stepGap('LazyFrame.drop'),
  stepGap('LazyFrame.drop_nans'),
  stepGap('LazyFrame.drop_nulls'),
  stepGap('LazyFrame.fill_nan'),
  stepGap('LazyFrame.first'),
  stepGap('LazyFrame.gather_every'),
  stepGap('LazyFrame.head'),
  stepGap('LazyFrame.last'),
  stepGap('LazyFrame.limit'),
  stepGap('LazyFrame.reverse'),
  stepGap('LazyFrame.slice'),
  stepGap('LazyFrame.tail'),
  stepGap('LazyFrame.with_row_index'),
  // The parameters of a step that its head does not draw and its options do not offer.
  {
    cover: { kind: 'stepParameter', entry: 'LazyFrame.sort', parameter: 'more_by' },
    reason:
      'the fields to order by are one list, so a second list of more fields is not offered (#137)',
  },
  {
    cover: { kind: 'stepParameter', entry: 'LazyFrame.join', parameter: 'on' },
    reason:
      'the keys of the two tables are one list each, so `on`, one list of keys both tables share, is not offered (#137)',
  },
  // An expression the walk gives no context.
  {
    cover: { kind: 'noContext', entry: 'LazyFrame.filter', parameter: 'constraints' },
    reason:
      'the engine walks the value of a keyword constraint of a filter and reports no context for it, so the editor cannot ask what may stand there (#179)',
  },
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
  { cover: { kind: 'typed', literal: 'flag' }, reason: NO_TYPED_FLAG },
  { cover: { kind: 'shown', what: 'literal flag' }, reason: FLAG_NOT_SHOWN },
  { cover: { kind: 'shown', what: 'resolution several' }, reason: SEVERAL_NOT_SHOWN },
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
