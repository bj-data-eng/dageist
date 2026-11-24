/**
 * @file The document the editor draws, and how it changes it. The host holds the node's document
 * and its revision; a window asks for both, draws them, and says what the person did as an edit
 * made against the revision it drew. The engine makes the edit, so a window never builds or
 * changes a document: it sends the edit and draws what the host answers with. Edits are sent one
 * after another, each against the revision the one before it gave, so an action that is several
 * edits, such as a join's table and then its right keys, is sent in order.
 *
 * When another window has changed the document, the host refuses an edit made against the old
 * revision. The window then asks for the document again, and the person does it again against
 * what is there.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import type {
  DocumentStateView,
  EditRequest,
  FrameRequest,
  Host,
  PlaceRequest,
  ProblemView,
  StartRequest,
  ValueRequest,
} from '@dageist/host';
import { HostError } from '@dageist/host';

import type { Path, StepAt } from './model/doc';
import { placeOf } from './model/doc';

/** The topic the windows tell each other that the document changed on. */
const TOPIC = 'document';

/** What an edit came to, for the part of the editor that sent it. */
export type Sent =
  /** The edit was made, and this is the document it gave. */
  | { readonly status: 'edited'; readonly state: DocumentStateView }
  /** The engine refused it, for this kind of reason, which the editor may decide on. */
  | {
      readonly status: 'refused';
      readonly problem: ProblemView;
      readonly message: string;
    }
  /** The document had changed since the edit was made, and has been asked for again. */
  | { readonly status: 'stale' }
  /** The host could not be asked. */
  | { readonly status: 'failed'; readonly reason: string };

/** What the editor holds of the document, and how it sends an edit. */
export interface Editor {
  /** The document, its revision and its report, or `null` until the host has sent them. */
  readonly state: DocumentStateView | null;
  /** Why the document could not be had, or `null`. */
  readonly failure: string | null;
  /** What the last edit that did not go through said, in words, or `null`. */
  readonly notice: string | null;
  /** Forgets the notice. */
  readonly dismiss: () => void;
  /** Sends an edit against the revision the document is at when its turn comes. */
  readonly send: (edit: EditRequest) => Promise<Sent>;
}

/** Returns what an error says, in words. */
function wordsOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Holds the node's document for one window: asks the host for it, asks again when another window
 * says it changed it, and sends edits in order.
 */
export function useEditor(host: Host): Editor {
  const [state, setState] = useState<DocumentStateView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // The revision the next edit is made against, which is the newest one this window has seen.
  const revision = useRef<number | null>(null);
  // The edits wait their turn here, so that each is made against what the one before it gave.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const live = useRef(true);

  const hold = useCallback((held: DocumentStateView): void => {
    if (!live.current || (revision.current !== null && held.revision < revision.current)) return;
    revision.current = held.revision;
    setState(held);
    setFailure(null);
  }, []);

  const load = useCallback(async (): Promise<void> => {
    try {
      hold(await host.nodeDocument());
    } catch (error) {
      if (live.current) setFailure(wordsOf(error));
    }
  }, [host, hold]);

  useEffect(() => {
    live.current = true;
    revision.current = null;
    void load();
    const stop = host.listen(TOPIC, (message) => {
      const said =
        typeof message === 'object' && message !== null && 'revision' in message
          ? message.revision
          : null;
      if (typeof said === 'number' && said > (revision.current ?? 0)) void load();
    });
    return () => {
      live.current = false;
      stop();
    };
  }, [host, load]);

  const send = useCallback(
    (edit: EditRequest): Promise<Sent> => {
      const run = async (): Promise<Sent> => {
        const against = revision.current;
        if (against === null) return { status: 'failed', reason: 'The steps are not loaded yet.' };
        try {
          const answer = await host.editDocument(against, edit);
          if (answer.status === 'edited') {
            hold(answer);
            host.broadcast(TOPIC, { revision: answer.revision });
            return { status: 'edited', state: answer };
          }
          // An option that starts with nothing is not a mistake: the editor asks for its value.
          if (live.current && answer.problem.kind !== 'needsValue') setNotice(answer.message);
          return { status: 'refused', problem: answer.problem, message: answer.message };
        } catch (error) {
          if (error instanceof HostError && error.kind === 'stale') {
            if (live.current) {
              setNotice(
                'The steps were changed in another window, so this was not done. Try again.',
              );
            }
            await load();
            return { status: 'stale' };
          }
          if (live.current) setNotice(wordsOf(error));
          return { status: 'failed', reason: wordsOf(error) };
        }
      };
      const next = queue.current.then(run, run);
      queue.current = next;
      return next;
    },
    [host, hold, load],
  );

  const dismiss = useCallback(() => {
    setNotice(null);
  }, []);

  return useMemo(
    () => ({ state, failure, notice, dismiss, send }),
    [state, failure, notice, dismiss, send],
  );
}

const EditorContext = createContext<Editor | null>(null);

export const EditorProvider = EditorContext.Provider;

/** Returns the editor of the window. It is only there below an `EditorProvider`. */
export function useEditorHandle(): Editor {
  const editor = useContext(EditorContext);
  if (editor === null) throw new Error('The editor is used outside an EditorProvider.');
  return editor;
}

/** The step being drawn: where it is in the document, and its name. */
const StepContext = createContext<StepAt | null>(null);

export const StepProvider = StepContext.Provider;

/** Returns the step being drawn. It is only there below a `StepProvider`. */
export function useStepAt(): StepAt {
  const step = useContext(StepContext);
  if (step === null) throw new Error('A step part is used outside a StepProvider.');
  return step;
}

/** The edits of the steps of the document, which name a step by where it is. */
export interface ListEdits {
  /** Adds a step of `entry`, started as the engine starts it on `frame`, as the `at`th. */
  readonly addStep: (at: number, entry: string, frame: FrameRequest) => Promise<Sent>;
  /** Makes the step `step` a step of `entry`, started afresh. */
  readonly swapStep: (step: StepAt, entry: string, frame: FrameRequest) => Promise<Sent>;
  readonly removeStep: (step: StepAt) => Promise<Sent>;
  /** Moves the step `step` to the position `to`. */
  readonly moveStep: (step: StepAt, to: number) => Promise<Sent>;
}

/** Returns the edits of the list of steps. */
export function useListEdits(): ListEdits {
  const { send } = useEditorHandle();
  return useMemo(
    () => ({
      addStep: async (at, entry, frame) => send({ kind: 'addStep', at, entry, frame }),
      swapStep: async (step, entry, frame) =>
        send({ kind: 'swapStep', step: placeOf(step, []), entry, frame }),
      removeStep: async (step) => send({ kind: 'removeStep', step: placeOf(step, []) }),
      moveStep: async (step, to) => send({ kind: 'moveStep', step: placeOf(step, []), to }),
    }),
    [send],
  );
}

/** The edits inside the step being drawn, which name a place by the way down from the step. */
export interface StepEdits {
  readonly setArgument: (path: Path, value: ValueRequest) => Promise<Sent>;
  readonly clearArgument: (path: Path) => Promise<Sent>;
  /** Adds the option `parameter` of the call at `path`, started as the engine starts it. */
  readonly addOption: (path: Path, parameter: string) => Promise<Sent>;
  /** Sets the value of `parameter` for the `item`th of the keys it follows. */
  readonly setForItem: (
    path: Path,
    parameter: string,
    item: number,
    value: ValueRequest,
  ) => Promise<Sent>;
  readonly insertItem: (path: Path, at: number, item: ValueRequest) => Promise<Sent>;
  readonly removeItem: (path: Path) => Promise<Sent>;
  /** Puts a new test, which the engine starts, into the list at `path`. */
  readonly insertTest: (path: Path, at: number) => Promise<Sent>;
  readonly insertEntry: (
    path: Path,
    at: number,
    key: ValueRequest,
    value: ValueRequest,
  ) => Promise<Sent>;
  readonly removeEntry: (path: Path) => Promise<Sent>;
  readonly setStart: (path: Path, start: StartRequest) => Promise<Sent>;
  readonly startWith: (path: Path, entry: string) => Promise<Sent>;
  readonly insertLink: (
    path: Path,
    at: number,
    entry: string,
    standing: PlaceRequest | null,
  ) => Promise<Sent>;
  readonly swapLink: (path: Path, entry: string, standing: PlaceRequest | null) => Promise<Sent>;
  readonly removeLink: (path: Path) => Promise<Sent>;
  readonly moveLink: (path: Path, to: number) => Promise<Sent>;
}

/** Returns the edits of the step being drawn. */
export function useStepEdits(): StepEdits {
  const { send } = useEditorHandle();
  const step = useStepAt();
  const at = step;
  return useMemo(
    () => ({
      setArgument: async (path, value) =>
        send({ kind: 'setArgument', place: placeOf(at, path), value }),
      clearArgument: async (path) => send({ kind: 'clearArgument', place: placeOf(at, path) }),
      addOption: async (path, parameter) =>
        send({ kind: 'addOption', call: placeOf(at, path), parameter }),
      setForItem: async (path, parameter, item, value) =>
        send({ kind: 'setForItem', call: placeOf(at, path), parameter, item, value }),
      insertItem: async (path, index, item) =>
        send({ kind: 'insertItem', list: placeOf(at, path), at: index, item }),
      removeItem: async (path) => send({ kind: 'removeItem', item: placeOf(at, path) }),
      insertTest: async (path, index) =>
        send({ kind: 'insertTest', list: placeOf(at, path), at: index }),
      insertEntry: async (path, index, key, value) =>
        send({ kind: 'insertEntry', pairs: placeOf(at, path), at: index, key, value }),
      removeEntry: async (path) => send({ kind: 'removeEntry', entry: placeOf(at, path) }),
      setStart: async (path, start) =>
        send({ kind: 'setStart', expression: placeOf(at, path), start }),
      startWith: async (path, entry) =>
        send({ kind: 'startWith', expression: placeOf(at, path), entry }),
      insertLink: async (path, index, entry, standing) =>
        send({ kind: 'insertLink', expression: placeOf(at, path), at: index, entry, standing }),
      swapLink: async (path, entry, standing) =>
        send({ kind: 'swapLink', link: placeOf(at, path), entry, standing }),
      removeLink: async (path) => send({ kind: 'removeLink', link: placeOf(at, path) }),
      moveLink: async (path, to) => send({ kind: 'moveLink', link: placeOf(at, path), to }),
    }),
    [send, at],
  );
}
