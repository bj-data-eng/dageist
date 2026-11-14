/**
 * @file The list a block opens: a search box, then what is on offer in
 * groups. Each row shows the thing as the block it would become and a word
 * on what it is. A row that cannot be picked says why. What is typed narrows
 * the offer, and the caller may offer the typed text itself as a value.
 */

import { useEffect, useRef, useState } from 'react';

import type { Look, Tone } from './Block';
import { cx } from './cx';

/** One row of an offer. */
export interface Offer {
  /** Tells the row from the others in its list. */
  readonly id: string;
  /** What the row's block says. */
  readonly label: string;
  readonly tone: Tone;
  readonly look: Look;
  /** A word on what the row is, such as its dtype. */
  readonly note: string;
  /** Why the row cannot be picked, or `null` when it can. */
  readonly reason: string | null;
  /** Whether the row is ticked, in a list that takes several. `null` in a list that takes one. */
  readonly ticked: boolean | null;
  /** Whether the row does something, such as adding a line. Enter is not meant for such a row. */
  readonly act: boolean;
  /** A control shown at the end of the row, such as the name a ticked field is given. */
  readonly extra: React.ReactNode;
  readonly onPick: () => void;
}

/** Rows that belong together, under one heading. */
export interface OfferGroup {
  /** The heading, or `null` for rows that need none. */
  readonly title: string | null;
  /** Whether the heading says how many rows there are. */
  readonly counted: boolean;
  readonly offers: readonly Offer[];
}

/** What an offer list shows and reports. */
export interface OfferListProps {
  /** What the search box says while it is empty. */
  readonly placeholder: string;
  /** Returns what is on offer for the text typed so far. */
  readonly groups: (typed: string) => readonly OfferGroup[];
  /** Whether the list stays open when a row is picked, as one that takes several does. */
  readonly keepOpen: boolean;
  /** Whether the list is wide enough for a control at the end of its rows. */
  readonly wide: boolean;
  /** Called when a row was picked and the list is done with. */
  readonly onDone: () => void;
}

/** Returns what tells a row from every other row of a list: its group and its id. */
function keyOf(group: OfferGroup, offer: Offer): string {
  return `${group.title ?? ''}\u0000${offer.id}`;
}

/** Returns the place of every row within its group. */
function placesOf(groups: readonly OfferGroup[]): ReadonlyMap<string, number> {
  const places = new Map<string, number>();
  for (const group of groups) {
    for (const [at, offer] of group.offers.entries()) places.set(keyOf(group, offer), at);
  }
  return places;
}

/** Shows what is on offer and picks from it by pointer or keyboard. */
export function OfferList({
  placeholder,
  groups,
  keepOpen,
  wide,
  onDone,
}: OfferListProps): React.JSX.Element {
  const [typed, setTyped] = useState('');
  /** The row the keyboard is on, or `null` for the first one Enter is meant for. */
  const [marked, setMarked] = useState<string | null>(null);
  // Where each row stood when the list opened or was last searched. The rows keep those places
  // while rows are ticked, so the row under the pointer does not move away from it.
  const [places, setPlaces] = useState(() => placesOf(groups('')));
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    search.current?.focus();
  }, []);

  const shown = groups(typed)
    .filter((group) => group.offers.length > 0)
    .map((group) => {
      const offers = [...group.offers];
      const placeOf = (offer: Offer): number =>
        places.get(keyOf(group, offer)) ?? Number.MAX_SAFE_INTEGER;
      offers.sort((left, right) => placeOf(left) - placeOf(right));
      return { ...group, offers };
    });
  const pickable = shown.flatMap((group) => group.offers).filter((offer) => offer.reason === null);
  const first = pickable.find((offer) => !offer.act) ?? pickable[0];
  const chosen = pickable.find((offer) => offer.id === marked) ?? first;

  const chosenId = chosen?.id ?? null;
  useEffect(() => {
    // The row the keyboard is on is kept in view.
    if (chosenId === null) return;
    list.current?.querySelector('.dg-chosen')?.scrollIntoView({ block: 'nearest' });
  }, [chosenId]);

  const pick = (offer: Offer): void => {
    if (offer.reason !== null) return;
    offer.onPick();
    if (!keepOpen) onDone();
  };
  const step = (by: number): void => {
    const at = chosen === undefined ? -1 : pickable.indexOf(chosen);
    const next = pickable[(at + by + pickable.length) % pickable.length];
    if (next !== undefined) setMarked(next.id);
  };

  return (
    <div className={cx('dg-offers', wide && 'dg-wide')}>
      <label className="dg-search">
        <input
          ref={search}
          type="text"
          aria-label={placeholder}
          placeholder={placeholder}
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.currentTarget.value);
            setMarked(null);
            setPlaces(placesOf(groups(event.currentTarget.value)));
          }}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              step(event.key === 'ArrowDown' ? 1 : -1);
            } else if (event.key === 'Enter' && chosen !== undefined) {
              event.preventDefault();
              pick(chosen);
            }
          }}
        />
      </label>
      <div ref={list} className="dg-offer-list">
        {shown.map((group, at) => (
          <section
            // Groups keep their order while the list is open, so a group's place identifies it.
            key={group.title ?? String(at)}
            aria-label={group.title ?? undefined}
            className="dg-group"
          >
            {group.title !== null && (
              <div className="dg-group-head">
                {group.title}
                {group.counted && <span>{group.offers.length}</span>}
              </div>
            )}
            {group.offers.map((offer) => (
              <div key={offer.id} className={cx('dg-offer', offer === chosen && 'dg-chosen')}>
                <button
                  type="button"
                  tabIndex={-1}
                  disabled={offer.reason !== null}
                  aria-pressed={offer.ticked ?? undefined}
                  className="dg-offer-pick"
                  onPointerDown={(event) => {
                    // A press on a row must not take the search box's focus.
                    event.preventDefault();
                  }}
                  onClick={() => {
                    pick(offer);
                  }}
                >
                  {offer.ticked !== null && <span aria-hidden="true" className="dg-tick" />}
                  <span data-tone={offer.tone} data-look={offer.look} className="dg-block">
                    {offer.label}
                  </span>
                  {/* The note stays beside a control at the end of the row, so a row always says what it is. */}
                  <span className="dg-offer-note">{offer.reason ?? offer.note}</span>
                </button>
                {offer.extra}
              </div>
            ))}
          </section>
        ))}
        {shown.length === 0 && <div className="dg-nothing">Nothing by that name.</div>}
      </div>
    </div>
  );
}
