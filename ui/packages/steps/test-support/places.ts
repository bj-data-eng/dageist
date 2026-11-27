/**
 * @file Test support: the grid test's statement of which words of the editor stand at a place and
 * which group of a list each entry is picked from. It is built from what the components call, the
 * groups a word lists, how an entry's parameters are set, the functions a chain starts from and the
 * parameters a step's head draws, but the components do not call it: they compose their words by
 * their structure, and this says what that structure puts at each place. It is not part of the
 * package's front door.
 */

import type {
  ContextView,
  EntryView,
  OfferPlaceView,
  ParameterView,
  PositionView,
} from '@dageist/host';

import type { Catalogue } from '../src/model/catalogue';
import {
  HEAD,
  START_FUNCTIONS,
  VERBS,
  conditionId,
  groupedHeadOf,
  verbId,
  verbOf,
} from '../src/model/catalogue';
import type { ParameterControl } from '../src/model/controls';
import { settingOf } from '../src/model/controls';
import { FIELD_ID, LITERAL_ID, OTHERWISE, THEN } from '../src/model/doc';
import { START } from '../src/model/place';
import type { Actions, Group, ListedWord, ValueDrawing } from '../src/model/words';
import { actionsOf, listsOf } from '../src/model/words';

/** Returns whether every argument a parameter holds has a drawing: none of its controls is `none`. */
export function isDrawn(control: ParameterControl): boolean {
  switch (control.shape) {
    case 'one':
      return control.one.control !== 'none';
    case 'columns':
      return true;
    case 'many':
      return control.each.control !== 'none';
    case 'pairs':
      return control.key.control !== 'none' && control.value.control !== 'none';
    case 'perItem':
      return control.each !== 'none';
    default:
      return control satisfies never;
  }
}

/**
 * Returns whether the editor draws the outer position of `context`: the head of some step draws a
 * parameter that stands there, or the head of a step that groups draws one of a call after `group_by`.
 */
function outerIsDrawn(catalogue: Catalogue, context: ContextView): boolean {
  const stands = (parameter: ParameterView): boolean =>
    parameter.position?.kind === 'outer' && parameter.position.name === context.outer;
  return (
    VERBS.some((verb) =>
      (catalogue.entries.get(verbId(verb))?.parameters ?? []).some(
        (parameter) =>
          Object.values<string>(HEAD[verb]).includes(parameter.name) && stands(parameter),
      ),
    ) ||
    [...catalogue.entries.values()].some((entry) =>
      entry.parameters.some(
        (parameter) =>
          entry.grouped !== null &&
          groupedHeadOf(entry).some((held) => held.name === parameter.name) &&
          stands(parameter),
      ),
    )
  );
}

/** Returns whether the editor draws the nested position of `context`: the drawing of some entry sets a parameter that stands there. */
function nestedIsDrawn(catalogue: Catalogue, nested: string): boolean {
  return [...catalogue.entries.values()].some((entry) =>
    entry.parameters.some(
      (parameter) =>
        parameter.position?.kind === 'nested' &&
        parameter.position.name === nested &&
        ['argument', 'option', 'entry'].includes(settingOf(entry, parameter).by),
    ),
  );
}

/**
 * Returns whether an argument holds the innermost position of `context`, which is where an
 * expression starts there: the position of an entry's parameter that is drawn beside the method's
 * name, or the step's first parameter, as `settingOf` says.
 */
function argumentHolds(catalogue: Catalogue, context: ContextView): boolean {
  const innermost: PositionView =
    context.nested === null
      ? { kind: 'outer', name: context.outer }
      : { kind: 'nested', name: context.nested };
  return [...catalogue.entries.values()].some((entry) =>
    entry.parameters.some(
      (parameter) =>
        parameter.position?.kind === innermost.kind &&
        parameter.position.name === innermost.name &&
        settingOf(entry, parameter).by === 'argument',
    ),
  );
}

/** What `wordsAt` worked out, by catalogue and place, as a grid asks it for every entry of every place. */
const REMEMBERED = new WeakMap<Catalogue, Map<string, readonly ListedWord[]>>();

/**
 * Returns the words of the editor that stand at a place of `context` and `kind` and list what the
 * engine allows there. Where an expression starts, a value slot stands wherever a step's head or
 * an option draws a parameter that holds one there, the word a chain starts from stands with it,
 * and the argument of a method stands where the position that holds the expression is held by an
 * argument of some entry, as the catalogue's parameter settings say.
 * After a value, a list adds a method after it and the name of a method that stands there swaps it.
 * Inside a condition the editor draws a `when` with its `then` and `otherwise` and lists nothing at
 * the places between them. Where no position of the context is drawn, no word stands.
 */
export function wordsAt(
  catalogue: Catalogue,
  context: ContextView,
  kind: OfferPlaceView['after']['kind'],
): readonly ListedWord[] {
  const key = `${context.outer}/${context.nested ?? ''}/${kind}`;
  const remembered = REMEMBERED.get(catalogue)?.get(key);
  if (remembered !== undefined) return remembered;
  const words = wordsOf(catalogue, context, kind);
  const held = REMEMBERED.get(catalogue) ?? new Map<string, readonly ListedWord[]>();
  held.set(key, words);
  REMEMBERED.set(catalogue, held);
  return words;
}

/** Works out the words that stand at a place; `wordsAt` remembers them. */
function wordsOf(
  catalogue: Catalogue,
  context: ContextView,
  kind: OfferPlaceView['after']['kind'],
): readonly ListedWord[] {
  const drawn =
    outerIsDrawn(catalogue, context) &&
    (context.nested === null || nestedIsDrawn(catalogue, context.nested));
  if (!drawn || kind === 'condition') return [];
  if (kind === 'value') return [{ word: 'insertion' }, { word: 'method', options: false }];
  return [
    { word: 'value', fromPath: true },
    { word: 'chainStart' },
    ...(argumentHolds(catalogue, context) ? [{ word: 'argument' } as const] : []),
  ];
}

/** How an entry is picked or made by the editor. */
export type Route =
  /** It is a row of this group of a list. */
  | { readonly route: 'group'; readonly group: Group }
  /** It is made with the function `via`: a `then` and an `otherwise` come with `when`. */
  | { readonly route: 'with'; readonly via: string }
  /** The editor has no way to pick or make it. */
  | { readonly route: 'none' };

/** The links the editor draws with `when`: the one `then` and the one `otherwise`. */
const MADE_WITH_WHEN: ReadonlySet<string> = new Set([THEN, OTHERWISE]);

/** Returns how the editor picks or makes `entry`. */
export function routeOf(entry: EntryView): Route {
  // A field is one the list of fields picks and a typed value one the typed values do.
  if (entry.id === FIELD_ID) return { route: 'group', group: 'fields' };
  if (entry.id === LITERAL_ID) return { route: 'group', group: 'values' };
  if (START_FUNCTIONS.has(entry.id)) return { route: 'group', group: 'functions' };
  if (MADE_WITH_WHEN.has(entry.id)) return { route: 'with', via: conditionId('when') };
  if (entry.applies === 'expression') return { route: 'group', group: 'methods' };
  if (entry.applies === 'frame' && verbOf(entry.id) !== null) {
    return { route: 'group', group: 'steps' };
  }
  return { route: 'none' };
}

/**
 * Returns whether a person can pick or make `entry` at `place`: a word stands there whose list holds
 * the group the entry is picked from, or, for a link made with a function, the function can be
 * picked where an expression starts in the same context and the place is inside a condition.
 */
export function canPick(catalogue: Catalogue, entry: EntryView, place: OfferPlaceView): boolean {
  const route = routeOf(entry);
  switch (route.route) {
    case 'group':
      return wordsAt(catalogue, place.context, place.after.kind).some((word) =>
        listsOf(word).includes(route.group),
      );
    case 'with': {
      const via = catalogue.entries.get(route.via);
      return (
        place.after.kind === 'condition' &&
        via !== undefined &&
        canPick(catalogue, via, { context: place.context, after: START })
      );
    }
    case 'none':
      return false;
    default:
      return route satisfies never;
  }
}

/**
 * Returns what can be done to the piece a value slot draws for `drawing`: a value drawn as a chain
 * is drawn by the chain, whose piece is the word it starts from, and one drawn as one word by the
 * slot, whose piece is a word, which a method can follow where it is a connected input or a typed value.
 */
export function actionsOfSlot(drawing: ValueDrawing, removable = true): Actions {
  switch (drawing.drawn) {
    case 'chain':
      return actionsOf('chainStart');
    case 'start':
      return actionsOf('startWord', removable);
    case 'word':
      return actionsOf('word', removable);
    default:
      return drawing satisfies never;
  }
}
