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
  StartRequest,
  ValueRequest,
} from '@dageist/host';

import type { Action, Session, Sent } from './editSession';
import { NOT_LOADED, createSession } from './editSession';
import type { Path, StepAt } from './model/doc';
import { placeOf } from './model/doc';

/** The topic the windows tell each other that the document changed on. */
const TOPIC = 'document';

/** What the editor holds of the document, and how it sends an action. */
export interface Editor {
  /** The document, its revision and its report, or `null` until the host has sent them. */
  readonly state: DocumentStateView | null;
  /** Why the document could not be had, or `null`. */
  readonly failure: string | null;
  /** What the last edit that did not go through said, in words, or `null`. */
  readonly notice: string | null;
  /** Forgets the notice. */
  readonly dismiss: () => void;
  /**
   * Makes the edits of one action, the first against `drawn`, the revision of the document the
   * person saw, and each after it against what the one before gave.
   */
  readonly act: (drawn: number | null, edits: Action) => Promise<Sent>;
}

/**
 * Holds the node's document for one window: asks the host for it, asks again when another window
 * says it changed it, and sends the person's actions in order.
 */
export function useEditor(host: Host): Editor {
  const [state, setState] = useState<DocumentStateView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const session = useRef<Session | null>(null);

  useEffect(() => {
    const made = createSession(
      {
        load: async () => host.nodeDocument(),
        edit: async (revision, edit) => host.editDocument(revision, edit),
      },
      {
        held: setState,
        failure: setFailure,
        notice: setNotice,
        edited: (revision) => {
          host.broadcast(TOPIC, { revision });
        },
      },
    );
    session.current = made;
    void made.load();
    const stop = host.listen(TOPIC, (message) => {
      const said =
        typeof message === 'object' && message !== null && 'revision' in message
          ? message.revision
          : null;
      if (typeof said === 'number') made.heard(said);
    });
    // A window that has been away, or whose host was restarted, may draw an old document: ask
    // again when it is looked at.
    const looked = (): void => {
      if (document.visibilityState === 'visible') void made.load();
    };
    window.addEventListener('focus', looked);
    document.addEventListener('visibilitychange', looked);
    return () => {
      window.removeEventListener('focus', looked);
      document.removeEventListener('visibilitychange', looked);
      made.close();
      session.current = null;
      stop();
    };
  }, [host]);

  const act = useCallback(async (drawn: number | null, edits: Action): Promise<Sent> => {
    const open = session.current;
    return open === null ? NOT_LOADED : open.act(drawn, edits);
  }, []);

  const dismiss = useCallback(() => {
    setNotice(null);
  }, []);

  return useMemo(
    () => ({ state, failure, notice, dismiss, act }),
    [state, failure, notice, dismiss, act],
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

/**
 * Returns the edits of the list of steps. Each is made against the revision of the document this
 * render draws, which is the one the person acted on.
 */
export function useListEdits(): ListEdits {
  const { act, state } = useEditorHandle();
  const drawn = state?.revision ?? null;
  return useMemo(
    () => ({
      addStep: async (at, entry, frame) => act(drawn, [{ kind: 'addStep', at, entry, frame }]),
      swapStep: async (step, entry, frame) =>
        act(drawn, [{ kind: 'swapStep', step: placeOf(step, []), entry, frame }]),
      removeStep: async (step) => act(drawn, [{ kind: 'removeStep', step: placeOf(step, []) }]),
      moveStep: async (step, to) => act(drawn, [{ kind: 'moveStep', step: placeOf(step, []), to }]),
    }),
    [act, drawn],
  );
}

/** The edits inside one step as the requests that make them; a place is the way down from the step. */
export interface StepRequests {
  readonly setArgument: (path: Path, value: ValueRequest) => EditRequest;
  readonly clearArgument: (path: Path) => EditRequest;
  /** Adds the option `parameter` of the call at `path`, started as the engine starts it. */
  readonly addOption: (path: Path, parameter: string) => EditRequest;
  /** Sets the value of `parameter` for the `item`th of the keys it follows. */
  readonly setForItem: (
    path: Path,
    parameter: string,
    item: number,
    value: ValueRequest,
  ) => EditRequest;
  readonly insertItem: (path: Path, at: number, item: ValueRequest) => EditRequest;
  readonly removeItem: (path: Path) => EditRequest;
  /** Puts a new test, which the engine starts, into the list at `path`. */
  readonly insertTest: (path: Path, at: number) => EditRequest;
  readonly insertEntry: (
    path: Path,
    at: number,
    key: ValueRequest,
    value: ValueRequest,
  ) => EditRequest;
  readonly removeEntry: (path: Path) => EditRequest;
  readonly setStart: (path: Path, start: StartRequest) => EditRequest;
  readonly startWith: (path: Path, entry: string) => EditRequest;
  readonly insertLink: (
    path: Path,
    at: number,
    entry: string,
    standing: PlaceRequest | null,
  ) => EditRequest;
  readonly swapLink: (path: Path, entry: string, standing: PlaceRequest | null) => EditRequest;
  readonly removeLink: (path: Path) => EditRequest;
  readonly moveLink: (path: Path, to: number) => EditRequest;
}

/** Returns the requests of the edits inside the step `at`. */
function requestsOf(at: StepAt): StepRequests {
  return {
    setArgument: (path, value) => ({ kind: 'setArgument', place: placeOf(at, path), value }),
    clearArgument: (path) => ({ kind: 'clearArgument', place: placeOf(at, path) }),
    addOption: (path, parameter) => ({ kind: 'addOption', call: placeOf(at, path), parameter }),
    setForItem: (path, parameter, item, value) => ({
      kind: 'setForItem',
      call: placeOf(at, path),
      parameter,
      item,
      value,
    }),
    insertItem: (path, index, item) => ({
      kind: 'insertItem',
      list: placeOf(at, path),
      at: index,
      item,
    }),
    removeItem: (path) => ({ kind: 'removeItem', item: placeOf(at, path) }),
    insertTest: (path, index) => ({ kind: 'insertTest', list: placeOf(at, path), at: index }),
    insertEntry: (path, index, key, value) => ({
      kind: 'insertEntry',
      pairs: placeOf(at, path),
      at: index,
      key,
      value,
    }),
    removeEntry: (path) => ({ kind: 'removeEntry', entry: placeOf(at, path) }),
    setStart: (path, start) => ({ kind: 'setStart', expression: placeOf(at, path), start }),
    startWith: (path, entry) => ({ kind: 'startWith', expression: placeOf(at, path), entry }),
    insertLink: (path, index, entry, standing) => ({
      kind: 'insertLink',
      expression: placeOf(at, path),
      at: index,
      entry,
      standing,
    }),
    swapLink: (path, entry, standing) => ({
      kind: 'swapLink',
      link: placeOf(at, path),
      entry,
      standing,
    }),
    removeLink: (path) => ({ kind: 'removeLink', link: placeOf(at, path) }),
    moveLink: (path, to) => ({ kind: 'moveLink', link: placeOf(at, path), to }),
  };
}

/** The edits of the step being drawn, each one action of the person. */
export type StepEdits = {
  readonly [Name in keyof StepRequests]: StepRequests[Name] extends (
    ...given: infer Given
  ) => EditRequest
    ? (...given: Given) => Promise<Sent>
    : never;
} & {
  /**
   * Makes several edits as one action, each against the revision the one before gave, and stops at
   * the first that does not go through.
   */
  readonly together: (build: (requests: StepRequests) => Action) => Promise<Sent>;
};

/**
 * Returns the edits of the step being drawn. Each is made against the revision of the document this
 * render draws, which is the one the person acted on.
 */
export function useStepEdits(): StepEdits {
  const { act, state } = useEditorHandle();
  const step = useStepAt();
  const drawn = state?.revision ?? null;
  return useMemo(() => {
    const make = requestsOf(step);
    return {
      setArgument: async (path, value) => act(drawn, [make.setArgument(path, value)]),
      clearArgument: async (path) => act(drawn, [make.clearArgument(path)]),
      addOption: async (path, parameter) => act(drawn, [make.addOption(path, parameter)]),
      setForItem: async (path, parameter, item, value) =>
        act(drawn, [make.setForItem(path, parameter, item, value)]),
      insertItem: async (path, index, item) => act(drawn, [make.insertItem(path, index, item)]),
      removeItem: async (path) => act(drawn, [make.removeItem(path)]),
      insertTest: async (path, index) => act(drawn, [make.insertTest(path, index)]),
      insertEntry: async (path, index, key, value) =>
        act(drawn, [make.insertEntry(path, index, key, value)]),
      removeEntry: async (path) => act(drawn, [make.removeEntry(path)]),
      setStart: async (path, start) => act(drawn, [make.setStart(path, start)]),
      startWith: async (path, entry) => act(drawn, [make.startWith(path, entry)]),
      insertLink: async (path, index, entry, standing) =>
        act(drawn, [make.insertLink(path, index, entry, standing)]),
      swapLink: async (path, entry, standing) => act(drawn, [make.swapLink(path, entry, standing)]),
      removeLink: async (path) => act(drawn, [make.removeLink(path)]),
      moveLink: async (path, to) => act(drawn, [make.moveLink(path, to)]),
      together: async (build) => act(drawn, build(make)),
    };
  }, [act, drawn, step]);
}
