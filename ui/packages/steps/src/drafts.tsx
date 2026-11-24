/**
 * @file What the person has begun and not finished. A document holds no pair, list item or option
 * that has no value yet, so the editor keeps it as its own state until it has one, and then sends
 * the edit that puts it in the document. Nothing here is the document's.
 */

import { createContext, useCallback, useContext, useMemo, useState } from 'react';

import type { ValueRequest } from '@dageist/host';

/** What has been begun. */
export type Draft =
  /** An option of a call, which waits for its value. */
  | { readonly kind: 'option' }
  /** An item of a list, which waits for its value. */
  | { readonly kind: 'item' }
  /** An entry of pairs, with the key and the value picked so far. */
  | {
      readonly kind: 'entry';
      readonly key: ValueRequest | null;
      readonly value: ValueRequest | null;
    };

/** The drafts of the editor, by where they would go. */
export interface Drafts {
  readonly get: (key: string) => Draft | undefined;
  readonly set: (key: string, draft: Draft) => void;
  readonly clear: (key: string) => void;
  /** Forgets every draft. */
  readonly reset: () => void;
}

const DraftsContext = createContext<Drafts | null>(null);

export const DraftsProvider = DraftsContext.Provider;

/** Holds the drafts of one window. */
export function useDraftStore(): Drafts {
  const [held, setHeld] = useState<ReadonlyMap<string, Draft>>(new Map());
  const set = useCallback((key: string, draft: Draft) => {
    setHeld((before) => new Map(before).set(key, draft));
  }, []);
  const clear = useCallback((key: string) => {
    setHeld((before) => {
      if (!before.has(key)) return before;
      const after = new Map(before);
      after.delete(key);
      return after;
    });
  }, []);
  const reset = useCallback(() => {
    setHeld((before) => (before.size === 0 ? before : new Map()));
  }, []);
  return useMemo(
    () => ({ get: (key) => held.get(key), set, clear, reset }),
    [held, set, clear, reset],
  );
}

/** Returns the drafts of the editor. They are only there below a `DraftsProvider`. */
export function useDrafts(): Drafts {
  const drafts = useContext(DraftsContext);
  if (drafts === null) throw new Error('A draft is used outside a DraftsProvider.');
  return drafts;
}

/** Returns the key a draft of `purpose` at `place` is held under. */
export function draftKey(purpose: Draft['kind'], stepId: string, way: string): string {
  return `${purpose}:${stepId}:${way}`;
}
