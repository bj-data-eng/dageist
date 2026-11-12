/**
 * @file A button that is either pressed or not: tabs, modes and other
 * one-of-several choices that stay visible.
 */

import { cx } from './cx';

/** What a toggle button shows and does. */
export interface ToggleButtonProps {
  readonly pressed: boolean;
  /** Stops the button from being pressed, for a choice that cannot change right now. */
  readonly disabled: boolean;
  readonly onPress: () => void;
  readonly children: React.ReactNode;
}

/** Shows a button that looks held down while `pressed`. */
export function ToggleButton({
  pressed,
  disabled,
  onPress,
  children,
}: ToggleButtonProps): React.JSX.Element {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={disabled}
      className={cx(
        'rounded-lg border bg-panel px-3 py-1 text-xs enabled:cursor-pointer disabled:opacity-50',
        pressed ? 'border-accent text-ink' : 'border-line text-muted hover:text-ink',
      )}
      onClick={onPress}
    >
      {children}
    </button>
  );
}
