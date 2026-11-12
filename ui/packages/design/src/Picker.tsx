/**
 * @file A swappable part: a button showing the current choice that opens a
 * menu of the alternatives. Every exchangeable thing in the application is
 * drawn with it, so "this can be swapped" always looks the same.
 */

import { useCallback, useState } from 'react';

import { Popover } from './Popover';
import { cx } from './cx';

/** One entry of a picker's menu. A reason marks an entry that cannot be picked. */
export interface Choice {
  readonly label: string;
  readonly current: boolean;
  readonly reason: string | null;
  readonly onPick: () => void;
}

/** How a picker is coloured: by what kind of thing it holds. */
export type PickerTone = 'verb' | 'column' | 'term' | 'operator' | 'plain';

const TONES: Readonly<Record<PickerTone, string>> = {
  verb: 'font-semibold',
  column: 'bg-column text-column-ink',
  term: 'font-mono text-[12.5px]',
  operator: 'font-mono text-[12.5px] font-semibold text-operator',
  plain: '',
};

/** What a picker shows and offers. */
export interface PickerProps {
  readonly label: string;
  readonly tone: PickerTone;
  readonly choices: readonly Choice[];
  /** Extra classes for the button, such as a colour for one particular choice. */
  readonly className?: string;
  /** Receives the button, for a caller that also uses it as a drag handle. */
  readonly buttonRef?: (element: HTMLButtonElement | null) => void;
}

/**
 * Shows `label` as a button that opens a menu of `choices`. At rest it reads
 * as plain text; pointing at it or focusing it shows that it can be pressed.
 */
export function Picker({
  label,
  tone,
  choices,
  className,
  buttonRef,
}: PickerProps): React.JSX.Element {
  const [open, setOpen] = useState(false);
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
      <button
        ref={keepButton}
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        className={cx(
          'inline-flex cursor-pointer items-center rounded-md px-1.5 text-[13px] leading-[22px] whitespace-nowrap',
          'outline-1 -outline-offset-1 outline-transparent hover:bg-hover hover:outline-line-strong focus-visible:outline-accent',
          open && 'bg-hover outline-accent',
          TONES[tone],
          className,
        )}
        onClick={() => {
          setOpen(!open);
        }}
      >
        {label}
      </button>
      {open && button !== null && (
        <Popover anchor={button} onDismiss={close}>
          <div role="menu" className="min-w-40">
            {choices.map((choice) => (
              <button
                key={choice.label}
                type="button"
                role="menuitem"
                disabled={choice.reason !== null}
                className={cx(
                  'flex w-full items-baseline gap-2.5 rounded-md px-2.5 py-0.5 text-left text-[13px] leading-[22px] whitespace-nowrap',
                  'enabled:cursor-pointer enabled:hover:bg-hover disabled:text-muted',
                  choice.current && 'text-accent',
                )}
                onClick={() => {
                  setOpen(false);
                  choice.onPick();
                }}
              >
                <span>{choice.label}</span>
                {choice.reason !== null && (
                  <span className="ml-auto text-[11px] text-muted">{choice.reason}</span>
                )}
              </button>
            ))}
          </div>
        </Popover>
      )}
    </>
  );
}
