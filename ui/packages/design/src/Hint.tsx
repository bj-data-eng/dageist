/**
 * @file A short note that appears beside something while the pointer rests
 * on it or it holds focus. It never takes the pointer, so it cannot get in
 * the way of what it describes.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import { cx } from './cx';

/** How long the pointer rests before the note appears, in milliseconds. */
const DELAY = 450;
/** The space kept between the note and what it describes, in pixels. */
const GAP = 6;
/** The space kept between the note and the window's edge, in pixels. */
const EDGE = 8;

/** Where the note's top-left corner goes, in window coordinates. */
interface Place {
  readonly left: number;
  readonly top: number;
}

/** What a hint says and what it is about. */
export interface HintProps {
  /** The lines of the note, most important first. */
  readonly lines: readonly string[];
  /** Draws the note in the code font, for text that is code. */
  readonly code: boolean;
  readonly children: React.ReactNode;
}

/** Shows `lines` beside `children` while the pointer rests on them. */
export function Hint({ lines, code, children }: HintProps): React.JSX.Element {
  const subject = useRef<HTMLSpanElement>(null);
  const note = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);
  const [place, setPlace] = useState<Place | null>(null);
  const timer = useRef<number | null>(null);

  const cancel = (): void => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  };
  const hide = (): void => {
    cancel();
    setShown(false);
    setPlace(null);
  };
  const showSoon = (): void => {
    cancel();
    timer.current = window.setTimeout(() => {
      setShown(true);
    }, DELAY);
  };

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  useLayoutEffect(() => {
    if (!shown || subject.current === null || note.current === null) return;
    const against = subject.current.getBoundingClientRect();
    const size = note.current.getBoundingClientRect();
    const above = against.top - GAP - size.height;
    setPlace({
      left: Math.max(EDGE, Math.min(against.left, window.innerWidth - size.width - EDGE)),
      top: above >= EDGE ? above : against.bottom + GAP,
    });
  }, [shown]);

  return (
    <span
      ref={subject}
      className="inline-flex"
      onPointerEnter={showSoon}
      onPointerLeave={hide}
      onPointerDown={hide}
      onFocus={showSoon}
      onBlur={hide}
    >
      {children}
      {shown &&
        lines.length > 0 &&
        createPortal(
          <div
            ref={note}
            role="tooltip"
            className={cx(
              'pointer-events-none fixed z-50 max-w-md rounded-md border border-line-strong bg-canvas px-2 py-1 text-xs leading-5 text-ink shadow-lg',
              code && 'font-mono text-[12px]',
            )}
            style={{
              left: place?.left ?? 0,
              top: place?.top ?? 0,
              opacity: place === null ? 0 : 1,
            }}
          >
            {lines.map((line, at) => (
              <div key={at} className={cx(at > 0 && 'text-muted')}>
                {line}
              </div>
            ))}
          </div>,
          document.body,
        )}
    </span>
  );
}
