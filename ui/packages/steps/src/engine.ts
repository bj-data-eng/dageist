/**
 * @file Asks the host for the rows after a step, and hands back what the engine reported, prepared
 * for display. The run of the document comes with the document itself, from the host, so what the
 * engine reported is always of the steps being drawn; only the rows are asked for on their own.
 * The request is held for a moment so that a burst of edits is one request, and an answer for a
 * revision that a newer edit has replaced is dropped. Until the engine has answered for the newest
 * revision, what it said of the last one is shown, and `settled` is false.
 */

import { createContext, useContext, useEffect, useRef, useState } from 'react';

import type { Host, RowsView } from '@dageist/host';
import { HostError } from '@dageist/host';

import { Latest } from './model/latest';
import type { Outcome } from './model/latest';
import { rowsFailure, rowsToShow } from './model/report';
import type { PreparedStep, RowsAnswer, RowsDisplay } from './model/report';

/** How long an edit is held for the next one, in milliseconds. */
const EDIT_DELAY = 150;

/** How many rows a preview asks for. */
const PREVIEW_ROWS = 200;

/** Whether a failure may pass if the request is asked again: the host was not there, which is not an answer of the engine's. */
function mayPass(error: unknown): boolean {
  return !(error instanceof HostError);
}

/** Whether both are the same failure, which asking again gave again: nothing is new to draw. */
function sameFailure<Answer>(held: Outcome<Answer>, found: Outcome<Answer>): boolean {
  return !held.ok && !found.ok && wordsOf(held.error) === wordsOf(found.error);
}

/** Returns what was the matter, in words. */
function wordsOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What the rows after the chosen step are asked for. */
interface RowsRequest {
  readonly revision: number;
  readonly step: string;
}

/** What the rows of a request were answered with. */
interface RowsAnswered extends RowsAnswer {
  /** The revision the rows were asked for. */
  readonly revision: number;
}

/** What a window draws of the rows after the chosen step. */
export interface RowsState {
  /** The rows, or why there are none; `null` until the engine has answered for this step. */
  readonly display: RowsDisplay | null;
  /** Whether what is drawn is for the steps as they are now. */
  readonly settled: boolean;
}

function keyOfRows(request: RowsRequest): string {
  return JSON.stringify([request.step, request.revision]);
}

/**
 * Asks the engine for the rows after the step `selectedId` of the document at `revision`, as the
 * document changes and another step is chosen. A step the engine did not accept has no rows to ask
 * for. What was answered for another step is not shown for this one.
 */
export function useRows(
  host: Host,
  revision: number | null,
  selectedId: string | null,
  step: PreparedStep | null,
): RowsState {
  const [answered, setAnswered] = useState<RowsAnswered | null>(null);
  const scheduler = useRef<Latest<RowsRequest, RowsView> | null>(null);
  const askable = revision !== null && selectedId !== null && step?.status === 'valid';
  // The rows that are wanted now, which a scheduler made for a new host asks for at once.
  const wanted = useRef<RowsRequest | null>(null);

  useEffect(() => {
    wanted.current = askable ? { revision, step: selectedId } : null;
    if (wanted.current !== null) scheduler.current?.submit(wanted.current);
  }, [askable, revision, selectedId]);

  useEffect(() => {
    const made = new Latest<RowsRequest, RowsView>({
      delay: EDIT_DELAY,
      keyOf: keyOfRows,
      retryable: mayPass,
      send: async (request) => host.rowsAfterStep(request.revision, request.step, PREVIEW_ROWS),
      deliver: (request, outcome) => {
        setAnswered((held) =>
          held !== null &&
          held.step === request.step &&
          held.revision === request.revision &&
          sameFailure(held.outcome, outcome)
            ? held
            : { revision: request.revision, step: request.step, outcome },
        );
      },
    });
    scheduler.current = made;
    if (wanted.current !== null) made.submit(wanted.current);
    return () => {
      made.stop();
      scheduler.current = null;
    };
  }, [host]);

  if (selectedId === null || step === null) return { display: null, settled: true };
  if (!askable) {
    // The engine did not accept the step, so there are no rows to ask for: say why.
    return {
      display: { kind: 'message', text: step.problem ?? rowsFailure('refused', '', step) },
      settled: true,
    };
  }
  return {
    display: rowsToShow(answered, selectedId, step),
    settled: answered?.step === selectedId && answered.revision === revision,
  };
}

/** What the step being drawn is: what the engine reported of it. */
const PreparedStepContext = createContext<PreparedStep | null>(null);

export const PreparedStepProvider = PreparedStepContext.Provider;

/** What the engine reported of the step being drawn, and the dtypes of its chains. */
export function usePreparedStep(): PreparedStep | null {
  return useContext(PreparedStepContext);
}
