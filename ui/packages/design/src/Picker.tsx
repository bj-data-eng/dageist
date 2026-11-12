/**
 * @file A swappable part: a button showing the current choice that opens a
 * menu of the alternatives. Every exchangeable thing in the application is
 * drawn with it, so "this can be swapped" always looks the same.
 */

import { useEffect, useRef, useState } from 'react';

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
  verb: 'border-line-strong bg-hover font-semibold',
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
  /** Receives the button, for a caller that also uses it as a drag handle. */
  readonly buttonRef?: (element: HTMLButtonElement | null) => void;
}

/** Shows `label` as a button that opens a menu of `choices`. */
export function Picker({ label, tone, choices, buttonRef }: PickerProps): React.JSX.Element {
  const [open, setOpen] = useState(false);
  const host = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const closeOutside = (event: PointerEvent): void => {
      if (event.target instanceof Node && host.current?.contains(event.target) !== true) {
        setOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', closeOutside);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('pointerdown', closeOutside);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [open]);

  return (
    <span ref={host} className="relative inline-flex">
      <button
        ref={buttonRef}
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        className={cx(
          'inline-flex cursor-pointer items-center gap-1 rounded-md border border-transparent px-1.5',
          'text-[13px] leading-[22px] whitespace-nowrap hover:border-accent hover:bg-hover',
          open && 'border-accent bg-hover',
          TONES[tone],
        )}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span>{label}</span>
        <span aria-hidden="true" className="text-[10px] text-accent">
          ▾
        </span>
      </button>
      {open && (
        <div
          role="menu"
          className="absolute top-[calc(100%+4px)] left-0 z-20 min-w-40 rounded-lg border border-line-strong bg-panel p-1 shadow-lg"
        >
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
      )}
    </span>
  );
}
