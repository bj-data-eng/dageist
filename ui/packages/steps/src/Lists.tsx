/**
 * @file Lists under a row. Whatever a step takes that is more than one thing
 * is a list beneath it: numbered lines behind a rail. A line can own lists
 * of its own, set in again and numbered from it.
 *
 * A row or a line offers the place its lists are drawn in. What owns a list
 * may be anywhere inside the row, however deep, and the list still lands
 * under that row. A chain that is a value is three dots where it stands; the
 * dots own a list, and opening one set of dots shuts any other on the same
 * row, so a row never shows two of them at once.
 */

import { createContext, useContext, useEffect, useLayoutEffect, useState } from 'react';
import { createPortal } from 'react-dom';

import { Block, cx } from '@dageist/design';

/**
 * Writes its number on every line of `list`, counted from `prefix`, and on
 * the lines of the lists under each line, counted from that line. A line
 * that waits has no number. Returns how many lines were numbered.
 */
function numberLines(list: Element, prefix: string, start: number): number {
  let count = start;
  for (const part of list.children) {
    if (part.classList.contains('dg-cases')) {
      // Cases are lines of the list they stand in, laid out in columns.
      count = numberLines(part, prefix, count);
      continue;
    }
    const line = part.querySelector(':scope > .dg-line');
    if (line === null || line.classList.contains('dg-waits')) continue;
    count += 1;
    const number = `${prefix}.${String(count)}`;
    line.querySelector(':scope > .dg-mark')?.setAttribute('data-number', number);
    // The lists under one line are numbered as one, so that no two lines share a number.
    let under = 0;
    for (const inner of part.querySelectorAll(':scope > .dg-lines')) {
      under = numberLines(inner, number, under);
    }
  }
  return count;
}

/**
 * Keeps the lines under `row` numbered from `prefix`, the row's own number,
 * whatever is added to them or taken away.
 */
export function useLineNumbers(row: HTMLElement | null, prefix: string): void {
  useEffect(() => {
    if (row === null) return undefined;
    const number = (): void => {
      let count = 0;
      for (const list of row.querySelectorAll(':scope > .dg-lines')) {
        count = numberLines(list, prefix, count);
      }
    };
    number();
    const watcher = new MutationObserver(number);
    watcher.observe(row, { childList: true, subtree: true });
    return () => {
      watcher.disconnect();
    };
  }, [row, prefix]);
}

/** Something just added, which is to be gone to as soon as it is shown. */
export interface Fresh {
  /** The id of what was just added, or `null`. */
  readonly id: string | null;
  /** Says that the thing with `id` was just added. */
  readonly mark: (id: string) => void;
}

/** What was just added, so that its first slot opens or its name is typed straight away. */
export const FreshContext = createContext<Fresh>({ id: null, mark: () => undefined });

/** What a row or line offers the things inside it that own lists. */
interface Owner {
  /** The element the lists that always show are drawn in, once it exists. */
  readonly outlet: HTMLElement | null;
  /** The element the list of whichever dots are open is drawn in, after the others. */
  readonly dotsOutlet: HTMLElement | null;
  /** The row itself, for telling which dots in it come first. */
  readonly row: HTMLElement | null;
  /** The dots whose list is open: `undefined` until any are pressed, `null` when all are shut. */
  readonly open: string | null | undefined;
  readonly setOpen: (id: string | null) => void;
}

const OwnerContext = createContext<Owner>({
  outlet: null,
  dotsOutlet: null,
  row: null,
  open: undefined,
  setOpen: () => undefined,
});

/** What a row that owns lists shows. */
export interface ListOwnerProps {
  /** Extra classes for the place the lists are drawn in, such as how far it is set in. */
  readonly outletClassName: string;
  /** Receives the element the row is drawn in. */
  readonly children: (setRow: (element: HTMLElement | null) => void) => React.ReactNode;
}

/** Draws a row and, under it, the lists of anything inside it. */
export function ListOwner({ outletClassName, children }: ListOwnerProps): React.JSX.Element {
  const [outlet, setOutlet] = useState<HTMLElement | null>(null);
  const [dotsOutlet, setDotsOutlet] = useState<HTMLElement | null>(null);
  const [row, setRow] = useState<HTMLElement | null>(null);
  const [open, setOpen] = useState<string | null | undefined>(undefined);
  return (
    <OwnerContext.Provider value={{ outlet, dotsOutlet, row, open, setOpen }}>
      {children(setRow)}
      {/* What the row always shows comes first, so it keeps its place when dots are opened. */}
      <div ref={setOutlet} className={cx('dg-lines', outletClassName)} />
      <div ref={setDotsOutlet} className={cx('dg-lines', outletClassName)} />
    </OwnerContext.Provider>
  );
}

/** What a line shows and reports. */
export interface LineProps {
  /** Takes the line away, or `null` when it cannot be taken away. */
  readonly onRemove: (() => void) | null;
  /** Whether the line is the faint one that waits to add the next. */
  readonly waits: boolean;
  readonly children: React.ReactNode;
}

/**
 * One line of a list: its number, then what it holds. Pointing at a line
 * that can be taken away turns its number into the cross that removes it.
 * A line can own lists of its own, drawn under it.
 */
export function Line({ onRemove, waits, children }: LineProps): React.JSX.Element {
  return (
    // The line and its lists are one item, so that the lists are numbered from the line and the
    // lines after it carry on from it.
    <div className="dg-item">
      <ListOwner outletClassName="">
        {(setRow) => (
          <div ref={setRow} className={cx('dg-line', waits && 'dg-waits')}>
            {onRemove === null ? (
              <span className="dg-mark" />
            ) : (
              <button
                type="button"
                aria-label="Remove this line"
                title="Remove this line"
                className="dg-mark"
                onClick={onRemove}
              />
            )}
            {children}
          </div>
        )}
      </ListOwner>
    </div>
  );
}

/** Draws `children` as a list under the nearest row, whatever else that row shows. */
export function OwnedLines({ children }: { readonly children: React.ReactNode }): React.ReactNode {
  const { outlet } = useContext(OwnerContext);
  return outlet === null ? null : createPortal(children, outlet);
}

/** What a set of dots is and holds. */
export interface DotsProps {
  /** Tells these dots from any others on the same row. */
  readonly id: string;
  /** What the dots stand for, for assistive technology. */
  readonly label: string;
  /** The word before the dots, when what owns the list has a name, as `when` does. */
  readonly word: string | null;
  /** The lines of the list. */
  readonly children: React.ReactNode;
}

/**
 * Three dots that stand where a list's owner is, and the list they own under
 * the nearest row. A click opens the list and shuts any other on that row.
 * Until any are pressed, the first dots on a row are the open ones, and dots
 * that have just been added open their list.
 */
export function Dots({ id, label, word, children }: DotsProps): React.JSX.Element {
  const { dotsOutlet, row, open, setOpen } = useContext(OwnerContext);
  const fresh = useContext(FreshContext);
  const [token, setToken] = useState<HTMLButtonElement | null>(null);
  // Dots that are first shown while something was just added are what was added.
  const [isNew] = useState(fresh.id !== null);
  useEffect(() => {
    if (isNew) setOpen(id);
  }, [isNew, setOpen, id]);
  useLayoutEffect(() => {
    if (open !== undefined || row === null || token === null) return;
    if (row.querySelector('[data-dots]') === token.parentElement) setOpen(id);
  }, [open, row, token, id, setOpen]);
  const isOpen = open === id;
  const toggle = (): void => {
    setOpen(isOpen ? null : id);
  };
  return (
    <>
      <span data-dots className="contents">
        {word !== null && <Block label={word} tone="function" look="plain" onPress={toggle} />}
        <Block
          label="…"
          tone="function"
          look="dots"
          describedAs={label}
          expanded={isOpen}
          buttonRef={setToken}
          onPress={toggle}
        />
      </span>
      {isOpen && dotsOutlet !== null && createPortal(children, dotsOutlet)}
    </>
  );
}
