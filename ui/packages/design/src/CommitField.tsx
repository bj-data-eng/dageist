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
  readonly onCommit: (value: string) => void;
}

/** Shows `value` in a field and calls `onCommit` when an edit is finished. */
export function CommitField({
  value,
  label,
  numeric,
  code,
  onCommit,
}: CommitFieldProps): React.JSX.Element {
  return (
    <input
      // Remounting on a new value discards any unfinished edit.
      key={value}
      type={numeric ? 'number' : 'text'}
      aria-label={label}
      defaultValue={value}
      size={Math.max(value.length, numeric ? 2 : 4)}
      className={cx(
        'rounded-md border border-line-strong bg-canvas px-1.5 text-[13px] leading-[22px] text-ink',
        'focus:border-accent focus:outline-none',
        numeric && 'w-13',
        code && 'font-mono text-[12.5px]',
      )}
      onBlur={(event) => {
        if (event.currentTarget.value !== value) onCommit(event.currentTarget.value);
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur();
      }}
    />
  );
}
