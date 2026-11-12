/**
 * @file A small text or number field that reports its value only when the
 * edit is finished: on Enter or when the field is left.
 */

import { cx } from './cx';

/** What a commit field shows and reports. */
export interface CommitFieldProps {
  readonly value: string;
  /** What the field is, for assistive technology. */
  readonly label: string;
  readonly numeric: boolean;
  /** Draws the value in the code font, for names and literals that are code. */
  readonly code: boolean;
  /** Reports whether typed text may be kept. Text that may not is put back as it was. */
  readonly accepts?: (typed: string) => boolean;
  readonly onCommit: (value: string) => void;
}

/** Shows `value` in a field and calls `onCommit` when an edit is finished. */
export function CommitField({
  value,
  label,
  numeric,
  code,
  accepts,
  onCommit,
}: CommitFieldProps): React.JSX.Element {
  return (
    <input
      // Remounting on a new value discards any unfinished edit.
      key={value}
      type="text"
      inputMode={numeric ? 'numeric' : 'text'}
      aria-label={label}
      defaultValue={value}
      // The field is as wide as what it holds, so it takes no more room than text.
      style={{ width: `calc(${String(Math.max(value.length, 1))}ch + 0.75rem)` }}
      className={cx(
        // At rest a field is a dashed underline; it becomes a box when it is used.
        'border-b border-dashed border-line-strong bg-transparent px-1 text-[13px] leading-[22px] text-ink',
        'hover:border-solid hover:border-muted focus:rounded-sm focus:border-solid focus:border-accent focus:bg-canvas focus:outline-none',
        code && 'font-mono text-[12.5px]',
      )}
      onInput={(event) => {
        // Grow with the text while it is being typed.
        const field = event.currentTarget;
        field.style.width = `calc(${String(Math.max(field.value.length, 1))}ch + 0.75rem)`;
      }}
      onBlur={(event) => {
        const field = event.currentTarget;
        if (field.value === value) return;
        if (accepts !== undefined && !accepts(field.value)) {
          field.value = value;
          field.style.width = `calc(${String(Math.max(value.length, 1))}ch + 0.75rem)`;
          return;
        }
        onCommit(field.value);
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur();
      }}
    />
  );
}
