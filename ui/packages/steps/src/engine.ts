/**
 * @file Asks the host to run the editor's steps and to give the rows after a
 * step, and hands back what the engine reported, prepared for display. The
 * steps are written as a document, held for a moment so that a burst of edits
 * is one request, and an answer for a document that a newer edit has replaced
 * is dropped. Until the engine has answered for the newest document, what it
 * said of the last one is shown, and `settled` is false.
 */

import { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';

import type { Host, RowsView, RunView } from '@dageist/host';

import { useCatalogue, useNode } from './EngineProvider';
import { translate } from './model/document';
import type { Translation } from './model/document';
import { Latest } from './model/latest';
import type { Outcome } from './model/latest';
import { prepareRun, rowsFailure, rowsToShow } from './model/report';
import type {
  Answered as AnsweredRun,
  PreparedRun,
  PreparedStep,
  RowsAnswer,
  RowsDisplay,
} from './model/report';
import type { Step } from './model/steps';

/** How long an edit is held for the next one, in milliseconds. */
const EDIT_DELAY = 150;

/** How many rows a preview asks for. */
const PREVIEW_ROWS = 200;

/** What the engine's report of the steps came to, for a window to draw. */
export interface RunState {
  /** The steps as the document the engine runs, and what became of each. */
  readonly translation: Translation;
  /** What the engine reported, prepared. */
  readonly prepared: PreparedRun;
  /** Whether the report is of the steps as they are now. */
  readonly settled: boolean;
  /** Why the host could not be asked, or `null`. */
  readonly failure: string | null;
}

/** What a document was answered with. */
interface Answered {
  readonly document: string;
  readonly outcome: Outcome<RunView>;
}

/** Returns what was the matter, in words. */
function wordsOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Runs `steps` on the engine as they change, and returns what it reported. */
export function useRun(host: Host, steps: readonly Step[]): RunState {
  const catalogue = useCatalogue();
  const node = useNode();
  const translation = useMemo(() => translate(catalogue, steps), [catalogue, steps]);
  const [answered, setAnswered] = useState<Answered | null>(null);
  const scheduler = useRef<Latest<string, RunView> | null>(null);
  // The document that is wanted now, which a scheduler made for a new host asks for at once.
  const wanted = useRef(translation.text);

  useEffect(() => {
    wanted.current = translation.text;
    scheduler.current?.submit(translation.text);
  }, [translation.text]);

  useEffect(() => {
    const made = new Latest<string, RunView>({
      delay: EDIT_DELAY,
      keyOf: (document) => document,
      send: async (document) => host.runDocument(document),
      deliver: (document, outcome) => {
        setAnswered({ document, outcome });
      },
    });
    scheduler.current = made;
    made.submit(wanted.current);
    return () => {
      made.stop();
      scheduler.current = null;
    };
  }, [host]);

  const ran = useMemo<AnsweredRun | null>(
    () =>
      answered?.outcome.ok === true
        ? { document: answered.document, run: answered.outcome.answer }
        : null,
    [answered],
  );
  const prepared = useMemo(
    () => prepareRun(steps, translation, ran, node.source.columns),
    [steps, translation, ran, node],
  );
  return {
    translation,
    prepared,
    settled: answered?.document === translation.text,
    failure: answered?.outcome.ok === false ? wordsOf(answered.outcome.error) : null,
  };
}

/** What the rows after the chosen step are asked for. */
interface RowsRequest {
  readonly document: string;
  readonly step: string;
}

/** What the rows of a request were answered with. */
interface RowsAnswered extends RowsAnswer {
  /** The document the rows were asked for. */
  readonly document: string;
}

/** What a window draws of the rows after the chosen step. */
export interface RowsState {
  /** The rows, or why there are none; `null` until the engine has answered for this step. */
  readonly display: RowsDisplay | null;
  /** Whether what is drawn is for the steps as they are now. */
  readonly settled: boolean;
}

function keyOfRows(request: RowsRequest): string {
  return JSON.stringify([request.step, request.document]);
}

/**
 * Asks the engine for the rows after the step `selectedId`, as the steps
 * change and another step is chosen. A step that cannot be written as a
 * document has no rows to ask for. What was answered for another step is
 * not shown for this one.
 */
export function useRows(host: Host, run: RunState, selectedId: string | null): RowsState {
  const { translation, prepared } = run;
  const [answered, setAnswered] = useState<RowsAnswered | null>(null);
  const scheduler = useRef<Latest<RowsRequest, RowsView> | null>(null);
  const index = prepared.steps.findIndex((step) => step.id === selectedId);
  const askable = selectedId !== null && translation.steps[index]?.kind === 'sent';
  // The rows that are wanted now, which a scheduler made for a new host asks for at once.
  const wanted = useRef<RowsRequest | null>(null);

  useEffect(() => {
    wanted.current =
      askable && selectedId !== null ? { document: translation.text, step: selectedId } : null;
    if (wanted.current !== null) scheduler.current?.submit(wanted.current);
  }, [askable, selectedId, translation.text]);

  useEffect(() => {
    const made = new Latest<RowsRequest, RowsView>({
      delay: EDIT_DELAY,
      keyOf: keyOfRows,
      send: async ({ document, step }) => host.rowsAfterStep(document, step, PREVIEW_ROWS),
      deliver: ({ document, step }, outcome) => {
        setAnswered({ document, step, outcome });
      },
    });
    scheduler.current = made;
    if (wanted.current !== null) made.submit(wanted.current);
    return () => {
      made.stop();
      scheduler.current = null;
    };
  }, [host]);

  const step: PreparedStep | null = prepared.steps[index] ?? null;
  if (selectedId === null || step === null) return { display: null, settled: true };
  if (!askable) {
    // The step is not in the document, so there is nothing to ask: say why.
    return {
      display: { kind: 'message', text: step.problem ?? rowsFailure('noSuchStep', '', step) },
      settled: true,
    };
  }
  return {
    display: rowsToShow(answered, selectedId, step),
    settled: answered?.step === selectedId && answered.document === translation.text,
  };
}

/** What the step being drawn is: what the engine reported of it. */
const PreparedStepContext = createContext<PreparedStep | null>(null);

export const PreparedStepProvider = PreparedStepContext.Provider;

/** The words of the editor that are the matter with the step being drawn, and the dtypes of its chains. */
export function usePreparedStep(): PreparedStep | null {
  return useContext(PreparedStepContext);
}
