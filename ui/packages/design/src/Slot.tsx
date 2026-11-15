/**
 * @file The slot: a place that takes a value. At rest it shows what it holds
 * as a word, or several things as several words. A click opens the list it
 * is filled from. A slot whose word names the piece it stands in also offers,
 * at the foot of that list, what can be done to the piece. A slot that holds
 * several folds them to the room on its line and counts the rest, so a long
 * list is cut short before it would wrap.
 */

import { useCallback, useContext, useEffect, useLayoutEffect, useState } from 'react';

import type { Look, Tone } from './Block';
import { Block } from './Block';
import { HintContext } from './Hint';
import type { ListAction, OfferGroup } from './OfferList';
import { OfferList } from './OfferList';
import { PieceContext } from './Piece';
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
  /**
   * Whether the slot's word names the piece it stands in, as a method's name
   * does. Its list then ends with what can be done to that piece.
   */
  readonly names?: boolean;
  /** Receives the slot's word, for a caller that drags by it or places a list against it. */
  readonly buttonRef?: (element: HTMLButtonElement | null) => void;
}

/** Shows what the slot holds as a word that opens the list it is filled from. */
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
  names = false,
  openNow = false,
  buttonRef,
}: SlotProps): React.JSX.Element {
  const [open, setOpen] = useState(openNow);
  const [button, setButton] = useState<HTMLButtonElement | null>(null);
  const note = useContext(HintContext);
  const piece = useContext(PieceContext);
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
  const actions: ListAction[] = [];
  if (names && button !== null) {
    for (const action of [piece.add, piece.remove]) {
      if (action === null) continue;
      actions.push({
        label: action.label,
        onPress: () => {
          action.onPress(button);
        },
      });
    }
  }
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
            about={note.lines}
            aboutCode={note.code}
            actions={actions}
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

/** One of the several things a slot holds: a word, or two words that belong together. */
export interface Entry {
  /** Tells the entry from the others in its slot. */
  readonly id: string;
  /** Whether the entry cannot be worked out, such as a field that is no longer there. */
  readonly wrong: boolean;
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
  /** What stands between the two words of an entry, such as an arrow. A space when not given. */
  readonly between?: string;
}

/** The room a count takes at the end of a folded slot. */
const COUNT_ROOM = 40;

/**
 * Returns the room the entries of `slot` have on their line: from where the
 * slot starts to the end of the line of code it is in, less whatever follows
 * the slot on that line.
 */
function roomFor(slot: HTMLElement): number {
  const flow = slot.closest('[data-dg-flow]');
  if (flow === null) return Number.POSITIVE_INFINITY;
  let after = 0;
  for (let next = slot.nextElementSibling; next !== null; next = next.nextElementSibling) {
    after += next.getBoundingClientRect().width;
  }
  return flow.getBoundingClientRect().right - after - slot.getBoundingClientRect().left;
}

/**
 * Folds `slot` to the room it has: shows as many of its entries as fit on
 * the line, and a count of the rest, so a long list is cut short before it
 * would wrap. The entries are measured, not counted, so long names fold
 * sooner than short ones.
 */
function fold(slot: HTMLElement): void {
  const entries = [...slot.querySelectorAll('[data-dg-entry]')];
  const count = slot.querySelector('[data-dg-count]');
  if (count === null) return;
  for (const entry of entries) entry.classList.remove('dg-extra');
  const widths = entries.map((entry) => entry.getBoundingClientRect().width);
  const needed = widths.reduce((sum, width) => sum + width, 0);
  // With every entry out of the way the slot stands where its line starts, which is where its
  // room has to be measured from.
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
  // A mistake that is folded away shows on the count that stands for it.
  const hidden = entries.slice(shown);
  count.classList.toggle(
    'dg-wrong',
    hidden.some((entry) => entry.querySelector('.dg-wrong') !== null),
  );
  count.textContent = `+${String(entries.length - shown)}`;
}

/**
 * Shows what the slot holds as words with commas between them, as a list is
 * written, folded to the room on its line with a count of the rest. Any of
 * them opens the list, which takes several and stays open.
 */
export function ManySlot({
  entries,
  empty,
  between = ' ',
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

  // What is drawn is folded after every drawing, and again whenever the line changes size.
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
        {entries.map((entry, at) => (
          <span key={entry.id} data-dg-entry className="dg-entry">
            {entry.blocks.map((block, part) => (
              // The words of an entry keep their places.
              <span key={part}>
                {part > 0 && <i className="dg-quiet">{between}</i>}
                <Block
                  label={block.label}
                  tone={block.tone}
                  look={block.look}
                  describedAs={`${describedAs}: ${block.label}`}
                  wrong={wrong || entry.wrong}
                  expanded={open}
                  onPress={toggle}
                />
              </span>
            ))}
            {at < entries.length - 1 && <i className="dg-quiet">,</i>}
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
