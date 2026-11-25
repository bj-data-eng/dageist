/**
 * @file What the editor can do with a piece of an expression, decided here and drawn by the
 * components. A word of the editor opens a list, and which groups of offers that list holds is
 * written here; which words stand at a place the engine was asked about, and which group an entry
 * of the catalogue is picked from, are written here too, so that a test can ask, for every place
 * the engine allows an entry at, whether the editor lists it where someone can pick it. How a value
 * is drawn, as a word, as a chain, or as the lines of a condition, and which actions a piece has,
 * are decided here as well.
 */

import type { AfterView, EntryView } from '@dageist/host';

import { START_FUNCTIONS, verbOf } from './catalogue';
import type { ExpressionView, Shape, TestView, WhenLayout } from './doc';
import { fieldNameOf, isElement, shapeOf, startCall, testOf, whenOf } from './doc';

/** A group of offers, as a list holds it. */
export type Group =
  /** The row that starts a chain of its own where a value is picked. */
  | 'chain'
  /** A number or text typed in, as the engine reads it. */
  | 'values'
  /** The connected values and lists. */
  | 'inputs'
  /** The fields of the rows. */
  | 'fields'
  /** The functions a chain may start from. */
  | 'functions'
  /** The parameters a method or a step can be given. */
  | 'options'
  /** The methods that may follow what comes before. */
  | 'methods'
  /** The steps that can be added or changed to. */
  | 'steps';

/** The connected inputs a list of values offers: the single values and the lists, never a table. */
export const OFFERED_INPUTS = ['value', 'list'] as const;

/** A word of the editor that opens a list. */
export type ListedWord =
  /** The word a chain starts from, which swaps the start. */
  | { readonly word: 'chainStart' }
  /** The name of a method, which swaps the method and sets its options. */
  | { readonly word: 'method'; readonly options: boolean }
  /** The list that adds a method after a piece. */
  | { readonly word: 'insertion' }
  /** The slot of a value: `fromPath` says whether the value is in the document, so that a function can start it. */
  | { readonly word: 'value'; readonly fromPath: boolean }
  /** The slot of a method's argument. */
  | { readonly word: 'argument' }
  /** The name of a step, which changes the step and sets its options. */
  | { readonly word: 'step'; readonly options: boolean }
  /** The list that adds a step. */
  | { readonly word: 'newStep' };

/** Returns the groups the list of `word` holds, in the order it shows them. */
export function listsOf(word: ListedWord): readonly Group[] {
  switch (word.word) {
    case 'chainStart':
      return ['functions', 'inputs', 'fields'];
    case 'method':
      return word.options ? ['options', 'methods'] : ['methods'];
    case 'insertion':
      return ['methods'];
    case 'value':
      return word.fromPath
        ? ['chain', 'values', 'inputs', 'fields', 'functions']
        : ['chain', 'values', 'inputs', 'fields'];
    case 'argument':
      return ['values', 'inputs', 'fields'];
    case 'step':
      return word.options ? ['options', 'steps'] : ['steps'];
    case 'newStep':
      return ['steps'];
    default:
      return word satisfies never;
  }
}

/** Whether the row that starts a chain of its own is shown, which it is while nothing is typed. */
export function listsChainRow(typed: string): boolean {
  return typed === '';
}

/** What an expression has at a place, without its dtype, shape or state. */
export type PlaceKind = AfterView['kind'];

/**
 * Returns the words of the editor that stand at a place of `kind` and list what the engine allows
 * there. At the start of an expression the words are the slot of a value, the word a chain starts
 * from and the argument of a method; after a value, a list adds a method after it and the name of a
 * method that stands there swaps it. Inside a condition the editor draws a `when` with its `then`
 * and `otherwise` and lists nothing at the places between them.
 */
export function wordsAt(kind: PlaceKind): readonly ListedWord[] {
  switch (kind) {
    case 'start':
      return [{ word: 'value', fromPath: true }, { word: 'chainStart' }, { word: 'argument' }];
    case 'value':
      return [{ word: 'insertion' }, { word: 'method', options: false }];
    case 'condition':
      return [];
    default:
      return kind satisfies never;
  }
}

/** How an entry is picked or made by the editor. */
export type Route =
  /** It is a row of this group of a list. */
  | { readonly route: 'group'; readonly group: Group }
  /** It is made with a function that is a row of this group: a `then` and an `otherwise` come with `when`. */
  | { readonly route: 'with'; readonly group: Group }
  /** The editor has no way to pick or make it. */
  | { readonly route: 'none' };

/** The links the editor draws with `when`: the one `then` and the one `otherwise`. */
const MADE_WITH_WHEN: ReadonlySet<string> = new Set(['When.then', 'Then.otherwise']);

/** The field and the typed value that stand for `col` and `lit`, which a list offers by the name or the number typed. */
const PICKED_AS: Readonly<Record<string, Group>> = {
  'polars.col': 'fields',
  'polars.lit': 'values',
};

/** Returns how the editor picks or makes `entry`. */
export function routeOf(entry: EntryView): Route {
  if (Object.hasOwn(PICKED_AS, entry.id)) {
    const group = PICKED_AS[entry.id];
    return group === undefined ? { route: 'none' } : { route: 'group', group };
  }
  if (START_FUNCTIONS.has(entry.id)) return { route: 'group', group: 'functions' };
  if (MADE_WITH_WHEN.has(entry.id)) return { route: 'with', group: 'functions' };
  if (entry.applies === 'expression') return { route: 'group', group: 'methods' };
  if (entry.applies === 'frame' && verbOf(entry.id) !== null) {
    return { route: 'group', group: 'steps' };
  }
  return { route: 'none' };
}

/**
 * Returns whether a person can pick or make `entry` at a place of `kind`: a word stands there
 * whose list holds the group the entry is picked from, or, for a link made with a function, the
 * function can be picked where an expression starts and the places inside it are the condition's.
 */
export function canPick(entry: EntryView, kind: PlaceKind): boolean {
  const route = routeOf(entry);
  switch (route.route) {
    case 'group':
      return wordsAt(kind).some((word) => listsOf(word).includes(route.group));
    case 'with':
      return (
        kind === 'condition' && wordsAt('start').some((word) => listsOf(word).includes(route.group))
      );
    case 'none':
      return false;
    default:
      return route satisfies never;
  }
}

/** What a value slot draws when it holds one word and not a chain. */
export type WordShape = Exclude<Shape, { readonly kind: 'chain' }>;

/** How a slot draws the value it holds. */
export type ValueDrawing =
  /** As the chain it is, which starts from a field or a function and has methods. */
  | { readonly drawn: 'chain' }
  /** As one word: nothing picked, a connected input, a field, or a number or text typed in. */
  | { readonly drawn: 'word'; readonly shape: WordShape };

/** What a slot with nothing picked draws. */
const UNSET: WordShape = { kind: 'unset' };

/**
 * Returns how a slot draws `expression`: a field and any other chain are written where they
 * stand, which needs the expression to be in the document, `inDocument`; a connected input, a
 * typed value and nothing at all are one word each.
 */
export function valueDrawing(expression: ExpressionView | null, inDocument: boolean): ValueDrawing {
  const shape = expression === null ? UNSET : shapeOf(expression);
  if (shape.kind === 'chain') {
    return inDocument ? { drawn: 'chain' } : { drawn: 'word', shape: UNSET };
  }
  if (shape.kind === 'column' && inDocument && expression !== null) return { drawn: 'chain' };
  return { drawn: 'word', shape };
}

/** How a chain is laid out, by what it starts from. */
export type ChainBody =
  /** A `when`, with its condition, its `then` and its `otherwise` on lines of their own. */
  | { readonly body: 'when'; readonly layout: WhenLayout }
  /** `element`, which stands for the values of one cell of a pivot. */
  | { readonly body: 'element' }
  /** A function with no word of its own, drawn as its name with what it was given. */
  | { readonly body: 'function' }
  /** A field, a connected input or nothing yet, drawn as one word. */
  | { readonly body: 'start' };

/** Returns how the chain `expression` is laid out. */
export function chainBodyOf(expression: ExpressionView): ChainBody {
  const layout = whenOf(expression);
  if (layout !== null) return { body: 'when', layout };
  const call = startCall(expression.start);
  if (call !== null && isElement(call)) return { body: 'element' };
  if (call !== null && fieldNameOf(call) === null) return { body: 'function' };
  return { body: 'start' };
}

/** How a line of a condition is drawn. */
export type LineDrawing =
  /** A field, a comparison and a value, as three words. */
  | { readonly drawn: 'condition'; readonly test: TestView }
  /** A chain of its own, which ends in a comparison. */
  | { readonly drawn: 'chain' };

/** Returns how the line `expression` of a condition is drawn, given the entries that compare. */
export function lineDrawing(
  expression: ExpressionView,
  comparisons: ReadonlySet<string>,
): LineDrawing {
  const test = testOf(expression, comparisons);
  return test === null ? { drawn: 'chain' } : { drawn: 'condition', test };
}

/** A piece the editor draws with actions of its own. */
export type PieceKind =
  /** The word a chain starts from, which takes the whole chain away. */
  | 'chainStart'
  /** The name of a method of a chain. */
  | 'method'
  /** A value drawn as one word. */
  | 'word'
  /** The name of a step. */
  | 'step'
  /** The word `all of`, which heads a condition. */
  | 'conditionHead'
  /** A line of a condition. */
  | 'conditionLine'
  /** An entry of pairs. */
  | 'entry';

/** What can be done to a piece from the list its word opens. */
export interface Actions {
  /** Whether a piece can be added after it: a method after a piece of an expression, a step after a step, a line after a line. */
  readonly add: boolean;
  /** Whether it can be taken away. */
  readonly remove: boolean;
}

/**
 * Returns what can be done to a piece of `kind`. A word that is one of several can be taken away
 * where `removable`, and otherwise only be unset. Nothing can be added after a word that is one
 * value, a connected input or a typed number or text, though the engine allows methods after it;
 * the list of declared gaps says so.
 */
export function actionsOf(kind: PieceKind, removable = true): Actions {
  switch (kind) {
    case 'chainStart':
    case 'method':
    case 'step':
    case 'conditionHead':
    case 'conditionLine':
      return { add: true, remove: true };
    case 'word':
      return { add: false, remove: removable };
    case 'entry':
      return { add: false, remove: true };
    default:
      return kind satisfies never;
  }
}

/**
 * Returns what can be done to the piece a slot draws for `drawing`: a value drawn as a chain has
 * the actions of the word it starts from, and one drawn as one word those of a word.
 */
export function actionsOfValue(drawing: ValueDrawing, removable = true): Actions {
  return actionsOf(drawing.drawn === 'chain' ? 'chainStart' : 'word', removable);
}
