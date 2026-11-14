/**
 * @file A floating panel placed against another element. It is drawn at the
 * top of the page, outside whatever scrolls or clips its anchor, and Floating
 * UI keeps it in the right place: beneath the anchor, or above it when there
 * is more room there, moved sideways to stay inside the window, and no taller
 * than the room it has. It stays against its anchor while the page scrolls
 * or changes size, and is not drawn while the anchor is scrolled out of view.
 * A wheel turned over it scrolls only what is in it. It goes away when the
 * pointer is pressed elsewhere or Escape is pressed.
 */

import { autoUpdate, flip, hide, offset, shift, size, useFloating } from '@floating-ui/react-dom';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';

/** The space kept between the panel and its anchor, in pixels. */
const GAP = 4;
/** The space kept between the panel and the window's edge, in pixels. */
const EDGE = 8;
/** The least height the panel is told it has, so that a list in it never shrinks to nothing. */
const LEAST_ROOM = 140;

/**
 * How the panel is placed. The room it has is written on it as
 * `--popover-room`, for what is inside it to shrink to.
 */
const PLACING = [
  offset(GAP),
  flip({ padding: EDGE }),
  shift({ padding: EDGE }),
  size({
    padding: EDGE,
    apply: ({ availableHeight, elements }) => {
      const room = Math.max(LEAST_ROOM, Math.floor(availableHeight));
      elements.floating.style.setProperty('--popover-room', `${String(room)}px`);
    },
  }),
  hide(),
];

/**
 * Reports whether a wheel turned by `delta` over `target` can still scroll
 * something between it and `panel`. When nothing can, the turn would carry
 * on to the page behind the panel.
 */
function scrollsWithin(panel: HTMLElement, target: EventTarget | null, delta: number): boolean {
  let element = target instanceof HTMLElement ? target : null;
  while (element !== null && panel.contains(element)) {
    const room = element.scrollHeight - element.clientHeight;
    const style = window.getComputedStyle(element).overflowY;
    if (room > 0 && (style === 'auto' || style === 'scroll')) {
      if (delta < 0 && element.scrollTop > 0) return true;
      if (delta > 0 && element.scrollTop < room - 1) return true;
    }
    element = element.parentElement;
  }
  return false;
}

/** What a popover is placed against and holds. */
export interface PopoverProps {
  /** The element the panel opens beneath, or above when there is more room there. */
  readonly anchor: HTMLElement;
  readonly onDismiss: () => void;
  readonly children: React.ReactNode;
}

/** Shows `children` in a floating panel against `anchor`. */
export function Popover({ anchor, onDismiss, children }: PopoverProps): React.JSX.Element {
  const [panel, setPanel] = useState<HTMLDivElement | null>(null);
  const { floatingStyles, isPositioned, middlewareData } = useFloating({
    placement: 'bottom-start',
    strategy: 'fixed',
    elements: { reference: anchor, floating: panel },
    middleware: PLACING,
    // The anchor moves when the page scrolls or changes size, and the panel goes with it.
    whileElementsMounted: autoUpdate,
  });

  useEffect(() => {
    const isInside = (target: EventTarget | null): boolean =>
      target instanceof Node && (panel?.contains(target) === true || anchor.contains(target));
    const onPointerDown = (event: PointerEvent): void => {
      if (!isInside(event.target)) onDismiss();
    };
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onDismiss();
    };
    // A wheel turned over the panel scrolls the panel's list and nothing else: at the end of
    // the list, or over a list too short to scroll, the page behind stays where it is.
    const onWheel = (event: WheelEvent): void => {
      if (panel !== null && !scrollsWithin(panel, event.target, event.deltaY)) {
        event.preventDefault();
      }
    };
    panel?.addEventListener('wheel', onWheel, { passive: false });
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      panel?.removeEventListener('wheel', onWheel);
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [anchor, onDismiss, panel]);

  // Until it is placed, and while its anchor is scrolled out of view, the panel is transparent
  // and out of the pointer's way. It is not hidden, so that something inside it can take the
  // focus as soon as it opens, and keeps it until the anchor comes back.
  const shown = isPositioned && middlewareData.hide?.referenceHidden !== true;
  return createPortal(
    <div
      ref={setPanel}
      className="dg-ground z-50 overflow-hidden rounded border border-line-strong bg-panel text-ink shadow-[0_8px_22px_rgb(0_0_0/55%)]"
      style={{
        ...floatingStyles,
        opacity: shown ? 1 : 0,
        pointerEvents: shown ? 'auto' : 'none',
      }}
    >
      {children}
    </div>,
    document.body,
  );
}
