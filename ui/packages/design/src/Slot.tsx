/**
 * @file The slot: a place that takes a value. At rest it shows what it holds
 * as a block, or several things as several blocks. A click opens the list it
 * is filled from. A slot that holds several folds them to the room it has on
 * its line and counts the rest, so a long list never wraps.
 */

import { useCallback, useEffect, useLayoutEffect, useState } from 'react';

import type { Look, Tone } from './Block';
import { Block } from './Block';
import type { OfferGroup } from './OfferList';
import { OfferList } from './OfferList';
import { Popover } from './Popover';

/** What every slot needs to open its list. */
interface Listing {
  /** What the slot is for, for assistive technology. */
  readonly describedAs: string;
  /** What the list's search box says while it is empty. */
  readonly placeholder: string;
  /** Returns what the slot can take, for the text typed so far. */
  readonly offers: (typed: string) => readonly OfferGroup[];
  /** Whether the list is wide enough for a control at the end of its rows. */
  readonly wide?: boolean;
  /** Whether what the slot holds cannot be worked out. */
  readonly wrong?: boolean;
  /** Whether to open the list as soon as the slot is shown, as for something just added. */
  readonly openNow?: boolean;
}

/** What a slot that holds one thing shows and offers. */
export interface SlotProps extends Listing {
  readonly label: string;
  readonly tone: Tone;
  readonly look: Look;
  /** Whether the slot is a faint word that waits where the next thing goes. */
  readonly waits?: boolean;
  /** Receives the slot's block, for a caller that drags by it. */
  readonly buttonRef?: (element: HTMLButtonElement | null) => void;
}

/** Shows what the slot holds as a block that opens the list it is filled from. */
export function Slot({
  label,
  tone,
  look,
  describedAs,
  placeholder,
  offers,
  wide = false,
  wrong = false,
  waits = false,
  openNow = false,
  buttonRef,
}: SlotProps): React.JSX.Element {
  const [open, setOpen] = useState(openNow);
  const [button, setButton] = useState<HTMLButtonElement | null>(null);
  const keepButton = useCallback(
    (element: HTMLButtonElement | null) => {
      setButton(element);
      buttonRef?.(element);
    },
    [buttonRef],
  );
  const close = useCallback(() => {
    setOpen(false);
  }, []);
  return (
    <>
      <Block
        label={label}
        tone={tone}
        look={look}
        describedAs={describedAs}
        wrong={wrong}
        waits={waits}
        expanded={open}
        buttonRef={keepButton}
        onPress={() => {
          setOpen(!open);
        }}
      />
      {open && button !== null && (
        <Popover anchor={button} onDismiss={close}>
          <OfferList
            placeholder={placeholder}
            groups={offers}
            keepOpen={false}
            wide={wide}
            onDone={close}
          />
        </Popover>
      )}
    </>
  );
}

/** One of the several things a slot holds: a block, or a pair of blocks that touch. */
export interface Entry {
  /** Tells the entry from the others in its slot. */
  readonly id: string;
  readonly blocks: ReadonlyArray<{
    readonly label: string;
    readonly tone: Tone;
    readonly look: Look;
  }>;
}

/** What a slot that holds several things shows and offers. */
export interface ManySlotProps extends Listing {
  readonly entries: readonly Entry[];
  /** What the slot says while it holds nothing. */
  readonly empty: string;
}

/** The room a count takes at the end of a folded slot, and the room a bracket takes after it. */
const COUNT_ROOM = 44;
const BRACKET_ROOM = 10;

/**
 * Returns the room the entries of `slot` have: from where the slot starts to
 * the end of the row it is in, less what stands at the row's end, the bracket
 * that closes its piece, and whatever follows the slot inside the piece.
 */
function roomFor(slot: HTMLElement): number {
  const flow = slot.closest('[data-dg-flow]');
  if (flow === null) return Number.POSITIVE_INFINITY;
  const end = flow.querySelector('[data-dg-flow-end]');
  const kept = end === null ? 0 : end.getBoundingClientRect().width + 14;
  let after = 0;
  for (let next = slot.nextElementSibling; next !== null; next = next.nextElementSibling) {
    after += next.getBoundingClientRect().width;
  }
  const limit = flow.getBoundingClientRect().right - kept - BRACKET_ROOM - after;
  return limit - slot.getBoundingClientRect().left;
}

/**
 * Folds `slot` to the room it has: shows as many of its entries as fit, and
 * a count of the rest. The entries are measured, not counted, so long names
 * fold sooner than short ones.
 */
function fold(slot: HTMLElement): void {
  const entries = [...slot.querySelectorAll('[data-dg-entry]')];
  const count = slot.querySelector('[data-dg-count]');
  if (count === null) return;
  for (const entry of entries) entry.classList.remove('dg-extra');
  // A hair stands between two entries.
  const widths = entries.map((entry) => entry.getBoundingClientRect().width + 1);
  const needed = widths.reduce((sum, width) => sum + width, 0);
  // With every entry out of the way the piece sits where it will once it is folded, which is
  // where its room has to be measured from.
  for (const entry of entries) entry.classList.add('dg-extra');
  count.classList.remove('dg-extra');
  const room = roomFor(slot);
  let shown = entries.length;
  if (needed > room) {
    let used = 0;
    shown = 0;
    for (const width of widths) {
      if (used + width > room - COUNT_ROOM) break;
      used += width;
      shown += 1;
    }
  }
  for (const entry of entries.slice(0, shown)) entry.classList.remove('dg-extra');
  count.classList.toggle('dg-extra', shown === entries.length);
  count.textContent = `+${String(entries.length - shown)}`;
}

/** Shows what the slot holds as blocks, folded to the room there is, that open its list. */
export function ManySlot({
  entries,
  empty,
  describedAs,
  placeholder,
  offers,
  wide = false,
  wrong = false,
  openNow = false,
}: ManySlotProps): React.JSX.Element {
  const [open, setOpen] = useState(openNow);
  const [slot, setSlot] = useState<HTMLSpanElement | null>(null);
  const close = useCallback(() => {
    setOpen(false);
  }, []);
  const toggle = (): void => {
    setOpen(!open);
  };

  // What is drawn is folded after every drawing, and again whenever the row changes size.
  useLayoutEffect(() => {
    if (slot !== null) fold(slot);
  });
  useEffect(() => {
    const flow = slot?.closest('[data-dg-flow]') ?? null;
    if (slot === null || flow === null) return undefined;
    const watcher = new ResizeObserver(() => {
      fold(slot);
    });
    watcher.observe(flow);
    return () => {
      watcher.disconnect();
    };
  }, [slot]);

  return (
    <>
      <span ref={setSlot} className="dg-many">
        {entries.map((entry) => (
          <span key={entry.id} data-dg-entry className="dg-pair">
            {entry.blocks.map((block, part) => (
              <Block
                // A pair's blocks keep their places.
                key={part}
                label={block.label}
                tone={block.tone}
                look={block.look}
                describedAs={`${describedAs}: ${block.label}`}
                wrong={wrong}
                expanded={open}
                onPress={toggle}
              />
            ))}
          </span>
        ))}
        {entries.length === 0 && (
          <Block
            label={empty}
            tone="value"
            look="none"
            describedAs={describedAs}
            wrong={wrong}
            expanded={open}
            onPress={toggle}
          />
        )}
        {/* The count of what is folded away. Its number is written when the slot is folded. */}
        <button
          type="button"
          data-dg-count
          data-tone="function"
          data-look="plain"
          aria-label={`${describedAs}: the rest`}
          aria-expanded={open}
          className="dg-block dg-extra"
          onClick={toggle}
        >
          +
        </button>
      </span>
      {open && slot !== null && (
        <Popover anchor={slot} onDismiss={close}>
          <OfferList
            placeholder={placeholder}
            groups={offers}
            keepOpen
            wide={wide}
            onDone={close}
          />
        </Popover>
      )}
    </>
  );
}
