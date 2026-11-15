/**
 * @file A word that only shows something more when it is pressed, such as
 * the values behind a dtype. It changes nothing.
 */

import { useCallback, useState } from 'react';

import { cx } from './cx';
import { Popover } from './Popover';

/** What a peek shows at rest and when it is pressed. */
export interface PeekProps {
  /** What the word is for, for assistive technology. */
  readonly describedAs: string;
  /** The lines shown when the word is pressed, most important first. */
  readonly lines: readonly string[];
  /** Whether the lines are code, and so set in the code font. */
  readonly code: boolean;
  /** What the word says. */
  readonly children: React.ReactNode;
}

/** Shows `children` as a word that opens `lines` beneath it when pressed. */
export function Peek({ describedAs, lines, code, children }: PeekProps): React.JSX.Element {
  const [button, setButton] = useState<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const close = useCallback(() => {
    setOpen(false);
  }, []);
  return (
    <>
      <button
        ref={setButton}
        type="button"
        aria-label={describedAs}
        aria-expanded={open}
        className="dg-peek"
        onClick={() => {
          setOpen(!open);
        }}
      >
        {children}
      </button>
      {open && button !== null && lines.length > 0 && (
        <Popover anchor={button} onDismiss={close}>
          <div className={cx('dg-about', code && 'dg-code-note')}>
            {lines.map((line, at) => (
              <div key={at}>{line}</div>
            ))}
          </div>
        </Popover>
      )}
    </>
  );
}
