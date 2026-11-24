/**
 * @file What the person has begun and not finished. A document holds no pair, list item or option
 * that has no value yet, so the editor keeps it as its own state until it has one, and then sends
 * the edit that puts it in the document. Nothing here is the document's.
 */

import { createContext, useCallback, useContext, useMemo, useState } from 'react';

import type { DocumentView, ValueRequest } from '@dageist/host';

import type { Path } from './model/doc';
import { callAt, givenOf, stepNamed } from './model/doc';

/** The parameter a draft would give a value to: in the step called `step`, at the way `path`. */
export interface DraftPlace {
  readonly step: string;
  /** The way to the parameter, whose last segment is the parameter and whose rest is its call. */
  readonly path: Path;
}

/** What has been begun, and where it would go. */
export type Draft = { readonly place: DraftPlace } &
  /** An option of a call, which waits for its value. */
  (
    | { readonly kind: 'option' }
    /** An item of a list, which waits for its value. */
    | { readonly kind: 'item' }
    /** An entry of pairs, with the key and the value picked so far. */
    | {
        readonly kind: 'entry';
        readonly key: ValueRequest | null;
        readonly value: ValueRequest | null;
      }
  );

/** The drafts of the editor, by where they would go. */
export interface Drafts {
  readonly get: (key: string) => Draft | undefined;
  readonly set: (key: string, draft: Draft) => void;
  readonly clear: (key: string) => void;
  /** Forgets the drafts for which `keep` says no. */
  readonly retain: (keep: (draft: Draft) => boolean) => void;
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
  const retain = useCallback((keep: (draft: Draft) => boolean) => {
    setHeld((before) => {
      const after = new Map([...before].filter(([, draft]) => keep(draft)));
      return after.size === before.size ? before : after;
    });
  }, []);
  return useMemo(
    () => ({ get: (key) => held.get(key), set, clear, retain }),
    [held, set, clear, retain],
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

/**
 * Returns whether a draft still has a place to go to in `document`: its step is there, and so is
 * the call that holds its parameter. An option that the document now gives is not a draft any more.
 */
export function stillWanted(document: DocumentView, draft: Draft): boolean {
  const found = stepNamed(document.steps, draft.place.step);
  if (found === null) return false;
  const { path } = draft.place;
  const last = path.at(-1);
  if (last?.kind !== 'parameter') return false;
  const call = callAt(found.step.call, path.slice(0, -1));
  if (call === null) return false;
  return draft.kind !== 'option' || givenOf(call, last.name) === undefined;
}
