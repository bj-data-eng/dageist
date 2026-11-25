/**
 * @file What the editor does with an edit the engine refused. The engine says why in words, and as
 * a kind, so that the editor never decides on the words: it tells the person the words, except
 * where the refusal is a question that the editor answers by asking for something.
 */

import type { ProblemView } from '@dageist/host';

/** What the editor does with a refused edit. */
export type Handling =
  /** It tells the person the engine's words. */
  | 'tell'
  /** It asks for the value the option needs, and tells nothing. */
  | 'askForValue';

/** Returns what the editor does with an edit refused for `problem`. */
export function handlingOf(problem: ProblemView): Handling {
  switch (problem.kind) {
    case 'needsValue':
      // An option that starts with nothing a document holds is not a mistake.
      return 'askForValue';
    case 'noStep':
    case 'stepChanged':
    case 'nothing':
    case 'doesNotFit':
    case 'wrongPlace':
    case 'notPerItem':
    case 'noDefault':
    case 'outOfRange':
    case 'unknownEntry':
    case 'noStart':
    case 'cannotAdd':
    case 'refused':
      return 'tell';
    default:
      return problem satisfies never;
  }
}
