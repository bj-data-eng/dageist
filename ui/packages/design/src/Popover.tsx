/**
 * @file A floating panel placed against another element. It is drawn at the
 * top of the page, outside whatever scrolls or clips its anchor, and stays
 * against its anchor while the page scrolls or changes size. While the
 * anchor is scrolled out of view the panel is not drawn, and it is back when
 * the anchor is. A wheel turned over it scrolls only what is in it. It goes
 * away when the pointer is pressed elsewhere or Escape is pressed.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

/** The space kept between the panel and its anchor, in pixels. */
const GAP = 4;
/** The space kept between the panel and the window's edge, in pixels. */
const EDGE = 8;

/** Where the panel's top-left corner goes, in window coordinates, and whether it is drawn. */
interface Place {
  readonly left: number;
  readonly top: number;
  /** Whether any of the anchor shows. A panel whose anchor is scrolled away is not drawn. */
  readonly shown: boolean;
}

/**
 * Reports whether any of `anchor` shows: inside the window and inside
 * everything that scrolls or clips it.
 */
function shows(anchor: HTMLElement): boolean {
  const box = anchor.getBoundingClientRect();
  let top = Math.max(box.top, 0);
  let bottom = Math.min(box.bottom, window.innerHeight);
  let left = Math.max(box.left, 0);
  let right = Math.min(box.right, window.innerWidth);
  for (let around = anchor.parentElement; around !== null; around = around.parentElement) {
    const style = window.getComputedStyle(around);
    if (style.overflowX === 'visible' && style.overflowY === 'visible') continue;
    const clip = around.getBoundingClientRect();
    top = Math.max(top, clip.top);
    bottom = Math.min(bottom, clip.bottom);
    left = Math.max(left, clip.left);
    right = Math.min(right, clip.right);
  }
  return bottom > top && right > left;
}

/** Returns where `panel` goes against `anchor`: beneath it, or above when there is no room. */
function placeAgainst(anchor: HTMLElement, panel: HTMLElement): Place {
  const against = anchor.getBoundingClientRect();
  const size = panel.getBoundingClientRect();
  const below = against.bottom + GAP;
  const fitsBelow = below + size.height <= window.innerHeight - EDGE;
  return {
    left: Math.max(EDGE, Math.min(against.left, window.innerWidth - size.width - EDGE)),
    top: fitsBelow ? below : Math.max(EDGE, against.top - GAP - size.height),
    shown: shows(anchor),
  };
}

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
  /** The element the panel opens beneath, or above when there is no room. */
  readonly anchor: HTMLElement;
  readonly onDismiss: () => void;
  readonly children: React.ReactNode;
}

/** Shows `children` in a floating panel against `anchor`. */
export function Popover({ anchor, onDismiss, children }: PopoverProps): React.JSX.Element {
  const panel = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<Place | null>(null);

  useLayoutEffect(() => {
    if (panel.current !== null) setPlace(placeAgainst(anchor, panel.current));
  }, [anchor]);

  useEffect(() => {
    const isInside = (target: EventTarget | null): boolean =>
      target instanceof Node &&
      (panel.current?.contains(target) === true || anchor.contains(target));
    const onPointerDown = (event: PointerEvent): void => {
      if (!isInside(event.target)) onDismiss();
    };
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onDismiss();
    };
    // The anchor moves when the page scrolls or changes size, and the panel goes with it. It is
    // placed again once for each frame the browser draws, however many times it is told.
    let waiting: number | null = null;
    const follow = (): void => {
      waiting ??= window.requestAnimationFrame(() => {
        waiting = null;
        if (panel.current !== null) setPlace(placeAgainst(anchor, panel.current));
      });
    };
    const onScroll = (event: Event): void => {
      // Scrolling the panel's own list is not the page moving.
      if (!(event.target instanceof Node && panel.current?.contains(event.target) === true)) {
        follow();
      }
    };
    // A wheel turned over the panel scrolls the panel's list and nothing else: at the end of
    // the list, or over a list too short to scroll, the page behind stays where it is.
    const held = panel.current;
    const onWheel = (event: WheelEvent): void => {
      if (held !== null && !scrollsWithin(held, event.target, event.deltaY)) event.preventDefault();
    };
    held?.addEventListener('wheel', onWheel, { passive: false });
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', follow);
    return () => {
      if (waiting !== null) window.cancelAnimationFrame(waiting);
      held?.removeEventListener('wheel', onWheel);
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', follow);
    };
  }, [anchor, onDismiss]);

  return createPortal(
    <div
      ref={panel}
      className="dg-ground fixed z-50 overflow-hidden rounded border border-line-strong bg-panel text-ink shadow-[0_8px_22px_rgb(0_0_0/55%)]"
      style={{
        left: place?.left ?? 0,
        top: place?.top ?? 0,
        // Until it is placed, and while its anchor is scrolled out of view, the panel is
        // transparent and out of the pointer's way. It is not hidden, so that what is typed in
        // it keeps the focus and is there again when the anchor comes back.
        opacity: place?.shown === true ? 1 : 0,
        pointerEvents: place?.shown === true ? 'auto' : 'none',
      }}
    >
      {children}
    </div>,
    document.body,
  );
}
