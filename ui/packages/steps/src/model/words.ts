/**
 * @file What the editor can do with a piece of an expression, decided here and drawn by the
 * components. A word of the editor opens a list, and which groups of offers that list holds is
 * written here, and the components make each group the model lists and no other. How a value is
 * drawn, as a word, as a chain, or as the lines of a condition, and which actions a piece has,
 * are decided here as well. Which words stand at which place of the engine's grid, and which group
 * an entry is picked from, is the grid test's own statement of the editor and is kept with it, in
 * the package's test support.
 */

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

/** What each kind of word may list, in the order it shows it: the groups its components make. */
export const GROUPS_OF_WORD = {
  chainStart: ['functions', 'inputs', 'fields'],
  method: ['options', 'methods'],
  insertion: ['methods'],
  value: ['chain', 'values', 'inputs', 'fields', 'functions'],
  argument: ['values', 'inputs', 'fields'],
  step: ['options', 'steps'],
  newStep: ['steps'],
} as const satisfies Readonly<Record<ListedWord['word'], readonly Group[]>>;

/** The groups a kind of word may list. */
export type GroupOfWord<Kind extends ListedWord['word']> = (typeof GROUPS_OF_WORD)[Kind][number];

/**
 * Returns the groups the list of `word` holds, in the order it shows them: those `GROUPS_OF_WORD`
 * gives its kind, less the options of a name that has none to set and the functions of a value that
 * is not in the document.
 */
export function listsOf(word: ListedWord): readonly Group[] {
  const groups: readonly Group[] = GROUPS_OF_WORD[word.word];
  switch (word.word) {
    case 'method':
    case 'step':
      return word.options ? groups : groups.filter((group) => group !== 'options');
    case 'value':
      return word.fromPath ? groups : groups.filter((group) => group !== 'functions');
    case 'chainStart':
    case 'insertion':
    case 'argument':
    case 'newStep':
      return groups;
    default:
      return word satisfies never;
  }
}

/** Whether the row that starts a chain of its own is shown, which it is while nothing is typed. */
export function listsChainRow(typed: string): boolean {
  return typed === '';
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
export interface Actions<Add extends boolean = boolean> {
  /** Whether a piece can be added after it: a method after a piece of an expression, a step after a step, a line after a line. */
  readonly add: Add;
  /** Whether it can be taken away. */
  readonly remove: boolean;
}

/**
 * Whether something can be added after each kind of piece. Nothing can be added after a word that
 * is one value, a connected input or a typed number or text, though the engine allows methods
 * after it; the list of declared gaps says so. The type of each entry is its value, so that a
 * component that draws no adding for a kind fails to compile when this table says it has one.
 */
const ADDS = {
  chainStart: true,
  method: true,
  step: true,
  conditionHead: true,
  conditionLine: true,
  word: false,
  entry: false,
} as const satisfies Readonly<Record<PieceKind, boolean>>;

/**
 * Returns what can be done to a piece of `kind`. A word that is one of several can be taken away
 * where `removable`, and otherwise only be unset.
 */
export function actionsOf<Kind extends PieceKind>(
  kind: Kind,
  removable = true,
): Actions<(typeof ADDS)[Kind]> {
  return { add: ADDS[kind], remove: kind === 'word' ? removable : true };
}

/**
 * Returns the adding a piece has where the model says nothing can be added after it, which is no
 * adding. It compiles only for a piece whose kind has none, so a component that draws none stops
 * compiling when the model gives that kind one.
 */
export function noAddition(_actions: Actions<false>): null {
  return null;
}
