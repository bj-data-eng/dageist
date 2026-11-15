/**
 * @file A short note about something: its dtype, where it comes from, what a
 * method does. Nothing appears when the thing is pointed at. The note is
 * handed to the word it is about, which shows it at the top of the list the
 * word opens.
 */

import { createContext, useMemo } from 'react';

/** A note about a word. */
export interface HintLines {
  /** The lines of the note, most important first. */
  readonly lines: readonly string[];
  /** Whether the note is code, and so set in the code font. */
  readonly code: boolean;
}

/** The note about the words inside, for the list of the word it is about. */
export const HintContext = createContext<HintLines>({ lines: [], code: false });

/** What a hint says and what it is about. */
export interface HintProps extends HintLines {
  readonly children: React.ReactNode;
}

/** Gives `lines` to the word in `children` to show at the top of its list. */
export function Hint({ lines, code, children }: HintProps): React.JSX.Element {
  const note = useMemo(() => ({ lines, code }), [lines, code]);
  return <HintContext.Provider value={note}>{children}</HintContext.Provider>;
}
