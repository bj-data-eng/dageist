/**
 * @file What the step editor's lists offer, and the helpers that build it.
 */

import type { Offer, OfferGroup, Tone } from '@dageist/design';

import type { Dtype, Frame } from './model/frame';
import { findColumn } from './model/frame';

/** Reports whether what was typed is found in any of `texts`. Nothing typed finds everything. */
export function matches(typed: string, ...texts: readonly string[]): boolean {
  const sought = typed.trim().toLowerCase();
  return sought === '' || texts.some((text) => text.toLowerCase().includes(sought));
}

/** Returns a row that shows `label` as a plain block in `tone`, changed by whatever is in `more`. */
export function offer(
  label: string,
  tone: Tone,
  note: string,
  onPick: () => void,
  more: Partial<Offer> = {},
): Offer {
  return {
    id: `${tone}:${label}`,
    label,
    tone,
    look: 'plain',
    note,
    reason: null,
    ticked: null,
    act: false,
    extra: null,
    onPick,
    ...more,
  };
}

/** Returns `offers` under a heading that says how many there are. */
export function counted(title: string, offers: readonly Offer[]): OfferGroup {
  return { title, counted: true, offers };
}

/** Returns `offers` under a heading, or under none. */
export function titled(title: string | null, offers: readonly Offer[]): OfferGroup {
  return { title, counted: false, offers };
}

/** How the fields of a frame are offered. */
export interface FieldOffering {
  /** Returns why a field of `dtype` cannot be picked, or `null` when it can. */
  readonly reasonFor: (dtype: Dtype) => string | null;
  /** The fields that are ticked, in a list that takes several. `null` in one that takes one. */
  readonly chosen: readonly string[] | null;
  /** Returns the control a ticked field has at the end of its row, or `null`. */
  readonly renderExtra: (name: string) => React.ReactNode;
  readonly onPick: (name: string) => void;
}

/** Lists the fields of `frame` that match what was typed, each with its exact dtype. */
export function fieldOffers(frame: Frame, typed: string, offering: FieldOffering): Offer[] {
  const { reasonFor, chosen, renderExtra, onPick } = offering;
  const pick = (name: string): (() => void) => {
    return () => {
      onPick(name);
    };
  };
  const found = frame
    .filter((column) => matches(typed, column.name))
    .map((column) =>
      offer(column.name, 'column', column.dtype, pick(column.name), {
        reason: reasonFor(column.dtype),
        ticked: chosen === null ? null : chosen.includes(column.name),
        extra: renderExtra(column.name),
      }),
    );
  if (chosen === null) return found;
  // A ticked field that is no longer in the rows is still listed, so that it can be unticked.
  const missing = chosen
    .filter((name) => findColumn(frame, name) === null && matches(typed, name))
    .map((name) =>
      offer(name, 'column', 'not in the rows', pick(name), {
        id: `missing:${name}`,
        ticked: true,
        extra: renderExtra(name),
      }),
    );
  // The fields already ticked come first, so they are in view however long the list is.
  found.sort((left, right) => Number(right.ticked) - Number(left.ticked));
  return [...missing, ...found];
}

/** Offers every field, one to pick, with nothing ruled out. */
export function anyField(onPick: (name: string) => void): FieldOffering {
  return { reasonFor: () => null, chosen: null, renderExtra: () => null, onPick };
}
