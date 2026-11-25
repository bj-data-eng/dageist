import { describe, expect, it } from 'vitest';

import type { ProblemView } from '@dageist/host';

import { handlingOf } from './edits';

// What the editor does with an edit the engine refused: it tells the person, except where the
// refusal is the engine asking for the value an option needs.
describe('what the editor does with a refused edit', () => {
  it('is to ask for the value where an option starts with nothing a document holds', () => {
    expect(handlingOf({ kind: 'needsValue', parameter: 'descending' })).toBe('askForValue');
  });

  it('is to tell the person for every other kind of refusal', () => {
    const others: readonly ProblemView[] = [
      { kind: 'noStep', steps: 2 },
      { kind: 'stepChanged', found: 'a' },
      { kind: 'nothing' },
      { kind: 'doesNotFit', found: { kind: 'step' } },
      { kind: 'wrongPlace', needs: 'call', found: { kind: 'step' } },
      { kind: 'notPerItem' },
      { kind: 'noDefault' },
      { kind: 'outOfRange', index: 3, len: 2 },
      { kind: 'unknownEntry', entry: 'Expr.nope' },
      { kind: 'noStart', entry: 'LazyFrame.head' },
      { kind: 'cannotAdd', reason: 'Polars refuses every other word' },
      { kind: 'refused', reason: 'the model refuses it' },
    ];
    for (const problem of others) expect(handlingOf(problem), problem.kind).toBe('tell');
  });
});
