/**
 * @file Where an expression stands, as the engine's tables name a place. The
 * catalogue gives each expression parameter its position, and the engine's
 * report gives the place of every link of an expression it walked. A request
 * names a place by the positions that hold the expression, outermost first,
 * and what it has so far; the engine works out the context from them, so the
 * editor never decides which context a slot stands in.
 */

import type {
  AfterView,
  ContextView,
  OfferPlaceView,
  PlaceRequest,
  PositionView,
} from '@dageist/host';

/** What an expression that has nothing yet has: a call would start it. */
export const START: AfterView = { kind: 'start' };

/** No position: where nothing is known of the way down from a step. */
export const NO_POSITIONS: readonly PositionView[] = [];

/** Returns `positions` followed by `position`, which a parameter inside the expression holds. */
export function within(
  positions: readonly PositionView[],
  position: PositionView | null,
): readonly PositionView[] {
  return position === null ? positions : [...positions, position];
}

/** Returns the positions that hold an expression in `context`: its outer position, and the nested one if it has one. */
export function positionsOf(context: ContextView): readonly PositionView[] {
  const outer: PositionView = { kind: 'outer', name: context.outer };
  return context.nested === null ? [outer] : [outer, { kind: 'nested', name: context.nested }];
}

/** Returns the request for the place the engine reported. */
export function placeRequest(place: OfferPlaceView): PlaceRequest {
  return { positions: positionsOf(place.context), after: place.after };
}

/** Returns the request for where a call would start an expression held at `positions`. */
export function startRequest(positions: readonly PositionView[]): PlaceRequest {
  return { positions, after: START };
}
