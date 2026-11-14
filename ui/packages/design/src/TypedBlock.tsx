/**
 * @file The block that is typed in: a name, a text value or a number. It is
 * a block like any other at rest and a field while it is used. It reports
 * its value only when the edit is finished, on Enter or when it is left, and
 * puts back what it held when what was typed cannot be taken.
 */

import { useEffect, useRef } from 'react';

import { cx } from './cx';

/** What is typed into a block: a name, text shown between quotation marks, or a number. */
export type TypedKind = 'name' | 'text' | 'number';

/** What a typed block shows and reports. */
export interface TypedBlockProps {
  readonly value: string;
  readonly kind: TypedKind;
  /** What the block is, for assistive technology. */
  readonly label: string;
  /** Whether what the block holds cannot be worked out. */
  readonly wrong?: boolean;
  /** Whether to start typing in the block as soon as it is shown. */
  readonly focusNow?: boolean;
  /** Reports whether typed text may be kept. Text that may not is put back as it was. */
  readonly accepts?: (typed: string) => boolean;
  readonly onCommit: (value: string) => void;
}

function widthOf(text: string): string {
  return `${String(Math.max(text.length, 1))}ch`;
}

/** Shows `value` in a block that can be typed in, and calls `onCommit` when an edit is finished. */
export function TypedBlock({
  value,
  kind,
  label,
  wrong = false,
  focusNow = false,
  accepts,
  onCommit,
}: TypedBlockProps): React.JSX.Element {
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!focusNow) return;
    field.current?.focus();
    field.current?.select();
  }, [focusNow]);

  return (
    <label data-kind={kind} className={cx('dg-type', wrong && 'dg-wrong')}>
      <input
        // Remounting on a new value discards any unfinished edit.
        key={value}
        ref={field}
        type="text"
        inputMode={kind === 'number' ? 'numeric' : 'text'}
        aria-label={label}
        defaultValue={value}
        spellCheck={false}
        style={{ width: widthOf(value) }}
        onInput={(event) => {
          // The field stays as wide as its text, and says so when the text cannot be taken.
          const typed = event.currentTarget;
          typed.style.width = widthOf(typed.value);
          typed.setAttribute(
            'aria-invalid',
            String(accepts !== undefined && !accepts(typed.value)),
          );
        }}
        onBlur={(event) => {
          const typed = event.currentTarget;
          if (typed.value === value) return;
          if (accepts !== undefined && !accepts(typed.value)) {
            typed.value = value;
            typed.style.width = widthOf(value);
            typed.setAttribute('aria-invalid', 'false');
            return;
          }
          onCommit(typed.value);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') event.currentTarget.value = value;
          if (event.key === 'Enter' || event.key === 'Escape') event.currentTarget.blur();
        }}
      />
    </label>
  );
}
