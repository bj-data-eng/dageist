/**
 * @file How a window sends its edits and takes the document: the part of the editor that is not
 * React. The host holds the document and its revision. An action is made against the revision of
 * the document the person saw when they did it, never the newest one the window holds, because the
 * places in an edit are built from what was drawn: against a newer document they would land on a
 * step the person never saw. An action that is several edits chains the revisions from the answers
 * and stops at the first edit that does not go through. What the host says of the document is
 * taken whatever revision it is at, since a host that has been restarted is at a lower one; only an
 * answer that a later request has overtaken is dropped.
 */

import type { DocumentStateView, EditAnswerView, EditRequest, ProblemView } from '@dageist/host';
import { HostError } from '@dageist/host';

/** What an action came to, for the part of the editor that did it. */
export type Sent =
  /** The edits were made, and this is the document the last of them gave. */
  | { readonly status: 'edited'; readonly state: DocumentStateView }
  /** The engine refused an edit, for this kind of reason, which the editor may decide on. */
  | {
      readonly status: 'refused';
      readonly problem: ProblemView;
      readonly message: string;
    }
  /** The document had changed since the action was made, and has been asked for again. */
  | { readonly status: 'stale' }
  /** The host could not be asked. */
  | { readonly status: 'failed'; readonly reason: string };

/** What the session asks of the host. */
export interface SessionPort {
  /** Asks for the document, its revision and its report. */
  readonly load: () => Promise<DocumentStateView>;
  /** Makes an edit against `revision`. */
  readonly edit: (revision: number, edit: EditRequest) => Promise<EditAnswerView>;
}

/** What the session tells its window. */
export interface SessionListener {
  /** The document the window is to draw. */
  readonly held: (state: DocumentStateView) => void;
  /** Why the document could not be had, or `null` once it has been. */
  readonly failure: (reason: string | null) => void;
  /** What an edit that did not go through said, in words. */
  readonly notice: (words: string) => void;
  /** An edit was made and the document is now at `revision`, for the other windows to be told. */
  readonly edited: (revision: number) => void;
}

/** One or more edits that make one action of the person. */
export type Action = readonly [EditRequest, ...EditRequest[]];

/** The document of a window and the edits made to it. */
export interface Session {
  /** Asks the host for the document, which is then the window's whatever its revision is. */
  readonly load: () => Promise<void>;
  /**
   * Makes `edits` one after another. The first is made against `drawn`, the revision of the
   * document the person saw, and each after it against the revision the one before gave. It stops
   * at the first that is not made. `null` is for a window that has no document yet.
   */
  readonly act: (drawn: number | null, edits: Action) => Promise<Sent>;
  /** The revision of the document the window holds, or `null` before it has one. */
  readonly revision: () => number | null;
  /** Says that another window has made the document be at `said`. */
  readonly heard: (said: number) => void;
  /** Ends the session: its window is gone and is told nothing more. */
  readonly close: () => void;
}

/** What went wrong, in words. */
function wordsOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The words for an edit made against a document that has since changed. */
const CHANGED_SINCE = 'The steps changed since you did this, so it was not done. Try again.';

/** Opens the session of one window over `port`, which tells `listener` what it learns. */
export function createSession(port: SessionPort, listener: SessionListener): Session {
  let held: number | null = null;
  let open = true;
  // Counts the requests whose answers decide what the window holds: a request for the document, and
  // an answer to an edit. An answer to a request for the document counts only if nothing came
  // after the request, so that one that overlaps another or arrives late cannot put back what a
  // newer answer replaced.
  let stamp = 0;
  // Each action waits for the one before it.
  let queue: Promise<unknown> = Promise.resolve();

  const hold = (state: DocumentStateView): void => {
    held = state.revision;
    if (!open) return;
    listener.held(state);
    listener.failure(null);
  };

  const load = async (): Promise<void> => {
    stamp += 1;
    const mine = stamp;
    try {
      const state = await port.load();
      if (mine === stamp) hold(state);
    } catch (error) {
      if (mine === stamp && open) listener.failure(wordsOf(error));
    }
  };

  const tell = (words: string): void => {
    if (open) listener.notice(words);
  };

  const run = async (drawn: number | null, edits: Action): Promise<Sent> => {
    if (drawn === null) return { status: 'failed', reason: 'The steps are not loaded yet.' };
    let against = drawn;
    let last: Sent = { status: 'failed', reason: 'Nothing was done.' };
    for (const edit of edits) {
      try {
        const answer = await port.edit(against, edit);
        if (answer.status === 'refused') {
          // An option that starts with nothing is not a mistake: the editor asks for its value.
          if (answer.problem.kind !== 'needsValue') tell(answer.message);
          return { status: 'refused', problem: answer.problem, message: answer.message };
        }
        stamp += 1;
        hold(answer);
        listener.edited(answer.revision);
        against = answer.revision;
        last = { status: 'edited', state: answer };
      } catch (error) {
        if (error instanceof HostError && error.kind === 'stale') {
          tell(CHANGED_SINCE);
          await load();
          return { status: 'stale' };
        }
        tell(wordsOf(error));
        // What was drawn may be old, as after the host was restarted: ask for the document again.
        await load();
        return { status: 'failed', reason: wordsOf(error) };
      }
    }
    return last;
  };

  return {
    load,
    act: (drawn, edits) => {
      const next = queue.then(
        () => run(drawn, edits),
        () => run(drawn, edits),
      );
      queue = next;
      return next;
    },
    revision: () => held,
    heard: (said) => {
      if (said !== held) void load();
    },
    close: () => {
      open = false;
    },
  };
}
