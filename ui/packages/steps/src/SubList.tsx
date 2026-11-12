/**
 * @file Collapsible sub-lists. Anything in a step that can hold a chain of
 * its own is shown as a small toggle where it sits, with its rows listed
 * under the step it belongs to. Collapsed, it shows a one-line summary.
 *
 * A row offers the place its sub-lists are drawn in. The toggle may be
 * anywhere inside the row, however deep, and its rows still land under it.
 */

import { createContext, useContext, useState } from 'react';
import { createPortal } from 'react-dom';

import { cx } from '@dageist/design';

/** The element the nearest row draws its sub-lists in, once it exists. */
const Outlet = createContext<HTMLElement | null>(null);

/** What a sub-list host shows. */
export interface SubListHostProps {
  /** Classes for the place the sub-lists are drawn in. */
  readonly outletClassName: string;
  /** The row whose sub-lists are drawn under it. */
  readonly children: React.ReactNode;
}

/** Draws `children` and, under them, the sub-lists of anything inside them. */
export function SubListHost({ outletClassName, children }: SubListHostProps): React.JSX.Element {
  const [outlet, setOutlet] = useState<HTMLElement | null>(null);
  return (
    <Outlet.Provider value={outlet}>
      {children}
      <div ref={setOutlet} className={cx(outletClassName, 'empty:hidden')} />
    </Outlet.Provider>
  );
}

/** One row of a sub-list. It can hold sub-lists of its own, drawn under it and further in. */
export function SubRow({ children }: { readonly children: React.ReactNode }): React.JSX.Element {
  return (
    <SubListHost outletClassName="mb-0.5 ml-3 border-l-2 border-line-strong pl-2">
      <div className="group/sub flex min-h-8 flex-wrap items-center gap-x-0.5 gap-y-1 py-0.5">
        {children}
      </div>
    </SubListHost>
  );
}

/** What an expandable slot shows. */
export interface ExpandableProps {
  /** What the sub-list is, such as `when`. */
  readonly label: string;
  /** One line that stands for the sub-list while it is collapsed. */
  readonly summary: string;
  /** Extra classes for the toggle, such as a colour. */
  readonly className: string;
  /** The rows of the sub-list. */
  readonly children: React.ReactNode;
}

/** Draws a toggle in place and, while it is open, `children` in the nearest row's sub-list area. */
export function Expandable({
  label,
  summary,
  className,
  children,
}: ExpandableProps): React.JSX.Element {
  const outlet = useContext(Outlet);
  const [open, setOpen] = useState(true);
  return (
    <>
      <button
        type="button"
        aria-expanded={open}
        title={open ? `Collapse ${label}` : `Expand ${label}`}
        className={cx(
          'inline-flex cursor-pointer items-center gap-1 rounded-md px-1.5 font-mono text-[12.5px] leading-[22px]',
          'outline-1 -outline-offset-1 outline-line-strong hover:bg-hover',
          className,
        )}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span aria-hidden="true" className="text-[9px] text-muted">
          {open ? '▼' : '▶'}
        </span>
        {label}
      </button>
      {!open && (
        <span className="max-w-[28rem] truncate px-1 font-mono text-[12.5px] text-muted">
          {summary}
        </span>
      )}
      {open && outlet !== null && createPortal(children, outlet)}
    </>
  );
}
