/**
 * @file A floating panel placed against another element. It is drawn at the
 * top of the page, outside whatever scrolls or clips its anchor, and goes
 * away when the pointer is pressed elsewhere, Escape is pressed, or the page
 * scrolls or changes size.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

/** The space kept between the panel and its anchor, in pixels. */
const GAP = 4;
/** The space kept between the panel and the window's edge, in pixels. */
const EDGE = 8;

/** Where the panel's top-left corner goes, in window coordinates. */
interface Place {
  readonly left: number;
  readonly top: number;
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
    if (panel.current === null) return;
    const against = anchor.getBoundingClientRect();
    const size = panel.current.getBoundingClientRect();
    const below = against.bottom + GAP;
    const fitsBelow = below + size.height <= window.innerHeight - EDGE;
    setPlace({
      left: Math.max(EDGE, Math.min(against.left, window.innerWidth - size.width - EDGE)),
      top: fitsBelow ? below : Math.max(EDGE, against.top - GAP - size.height),
    });
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
    const onScroll = (event: Event): void => {
      // Scrolling the panel's own list is not the page moving.
      if (!(event.target instanceof Node && panel.current?.contains(event.target) === true)) {
        onDismiss();
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onDismiss);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onDismiss);
    };
  }, [anchor, onDismiss]);

  return createPortal(
    <div
      ref={panel}
      className="fixed z-50 rounded-lg border border-line-strong bg-panel p-1 text-ink shadow-lg"
      style={{
        left: place?.left ?? 0,
        top: place?.top ?? 0,
        // Until it is placed the panel is transparent, not hidden, so that
        // something inside it can take focus as soon as it opens.
        opacity: place === null ? 0 : 1,
      }}
    >
      {children}
    </div>,
    document.body,
  );
}
