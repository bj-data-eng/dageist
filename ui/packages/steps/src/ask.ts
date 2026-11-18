/**
 * @file Asks the engine what a list may offer, only while the list is open.
 * Each list has its own scheduler: the request is held for a moment so that
 * typing is one request, at most one is out at a time, the answer to a
 * request that a newer one has replaced is dropped, and a request that failed
 * because the host was not there is asked again, as the editor's runs are. A
 * list shows that it is waiting, not what was answered for another place.
 */

import { useEffect, useMemo, useRef, useState } from 'react';

import type {
  CandidateRequest,
  FitsRequest,
  FitsView,
  Host,
  NodeView,
  OffersView,
  PlaceRequest,
  StepStartsRequest,
  TypedKind,
  TypedRequest,
} from '@dageist/host';
import { HostError } from '@dageist/host';

import { useHost } from './EngineProvider';
import type { Outcome } from './model/latest';
import { Latest } from './model/latest';
import type { Frame } from './model/frame';
import { tablesOf } from './model/inputs';
import { candidateKey, prepareFits, prepareOptions, prepareReadings } from './model/offers';
import { startsByEntry } from './model/started';
import type { StepStarts } from './model/started';
import type { PreparedFit, PreparedOption, PreparedReading } from './model/offers';

/** How long a request is held for the next one, in milliseconds. */
const ASK_DELAY = 120;

/** Whether a failure may pass if the request is asked again: the host was not there, which is not an answer of the engine's. */
function mayPass(error: unknown): boolean {
  return !(error instanceof HostError);
}

/** What asking the engine for a list has come to. */
export type Asked<Answer> =
  /** Nothing is asked: the list is closed, or the place is not known. */
  | { readonly state: 'idle' }
  /** The engine has not answered for this request yet. */
  | { readonly state: 'waiting' }
  | { readonly state: 'ready'; readonly answer: Answer }
  /** The engine could not be asked, or refused to answer. */
  | { readonly state: 'failed'; readonly reason: string };

const IDLE = { state: 'idle' } as const;
const WAITING = { state: 'waiting' } as const;

/** Returns what an error says, in words. */
function wordsOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What a request was answered with, by the request's text. */
interface Held<Answer> {
  readonly key: string;
  readonly outcome: Outcome<Answer>;
}

/**
 * Asks `send` for `request` while it is not `null`, and says what the engine
 * answered for the newest request: nothing while it is `null`, and waiting
 * until the newest has been answered.
 */
export function useAsk<Request, Answer>(
  request: Request | null,
  send: (host: Host, request: Request) => Promise<Answer>,
): Asked<Answer> {
  const host = useHost();
  const key = request === null ? null : JSON.stringify(request);
  const [held, setHeld] = useState<Held<Answer> | null>(null);
  const scheduler = useRef<Latest<Request, Answer> | null>(null);
  // What is wanted now, which a scheduler made for a new host asks for at once.
  const wanted = useRef<Request | null>(request);
  const sender = useRef(send);

  useEffect(() => {
    sender.current = send;
  });

  useEffect(() => {
    wanted.current = request;
    if (request !== null) scheduler.current?.submit(request);
    // The request is the same whenever its text is.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  useEffect(() => {
    const made = new Latest<Request, Answer>({
      delay: ASK_DELAY,
      keyOf: (asked) => JSON.stringify(asked),
      retryable: mayPass,
      send: async (asked) => sender.current(host, asked),
      deliver: (asked, outcome) => {
        setHeld({ key: JSON.stringify(asked), outcome });
      },
    });
    scheduler.current = made;
    if (wanted.current !== null) made.submit(wanted.current);
    return () => {
      made.stop();
      scheduler.current = null;
    };
  }, [host]);

  if (key === null) return IDLE;
  if (held === null || held.key !== key) return WAITING;
  return held.outcome.ok
    ? { state: 'ready', answer: held.outcome.answer }
    : { state: 'failed', reason: wordsOf(held.outcome.error) };
}

/** Asks which calls may stand at `place`, while it is not `null`. */
export function useOffers(place: PlaceRequest | null): Asked<OffersView> {
  return useAsk(place, async (host, asked) => host.offersAt(asked));
}

/** What an entry's options are asked beside. */
export interface OptionsAsk {
  readonly entry: string;
  readonly chosen: ReadonlyArray<{ readonly parameter: string; readonly word: string }>;
}

/** Asks what the options of an entry may be set to, beside the options chosen, while it is not `null`. */
export function useOptions(ask: OptionsAsk | null): Asked<ReadonlyMap<string, PreparedOption>> {
  const asked = useAsk(ask, async (host, request) =>
    prepareOptions(await host.optionOffers(request.entry, request.chosen)),
  );
  return asked;
}

/**
 * Asks what each step starts with when it is added to the table `frame` with the connected
 * tables of `node`, while `open`: the engine decides every argument.
 */
export function useStepStarts(open: boolean, frame: Frame, node: NodeView): Asked<StepStarts> {
  const request: StepStartsRequest | null = open
    ? {
        columns: frame.map((column) => column.name),
        tables: tablesOf(node).map((table) => table.name),
      }
    : null;
  return useAsk(request, async (host, asked) => startsByEntry(await host.stepStarts(asked)));
}

/** What the fits of candidates are asked about. */
export interface FitSlot {
  /** The positions that hold the expression a candidate would start, or `null`. */
  readonly start: FitsRequest['start'];
  /** The argument a candidate would be given, or `null`. */
  readonly argument: FitsRequest['argument'];
}

/** What the engine said of the candidates of a request, by candidate. */
export type Fits = ReadonlyMap<string, PreparedFit>;

/** Asks which of `candidates` fit `slot`, while both are given. */
function useFitsOf(
  slot: FitSlot | null,
  candidates: readonly CandidateRequest[] | null,
): Asked<Fits> {
  const request = useMemo<FitsRequest | null>(
    () =>
      slot === null || candidates === null || candidates.length === 0
        ? null
        : { start: slot.start, argument: slot.argument, candidates },
    [slot, candidates],
  );
  return useAsk<FitsRequest, Fits>(request, async (host, asked) => {
    const view: FitsView = await host.candidateFits(asked);
    return prepareFits(asked.candidates, view, asked.start !== null);
  });
}

/** Returns the fields of `frame` and the connected values and lists of `node` as candidates, one for each dtype. */
export function listedCandidates(frame: Frame, node: NodeView): readonly CandidateRequest[] {
  const found = new Map<string, CandidateRequest>();
  for (const column of frame) {
    const candidate: CandidateRequest = { kind: 'field', dtype: column.dtype };
    found.set(candidateKey(candidate), candidate);
  }
  for (const input of node.inputs) {
    if (input.kind === 'table') continue;
    const candidate: CandidateRequest = {
      kind: 'input',
      dtype: input.dtype,
      list: input.kind === 'list',
    };
    found.set(candidateKey(candidate), candidate);
  }
  return [...found.values()];
}

/** What a list asks about its candidates, and what it was told. */
export interface CandidateFits {
  /** The fields and the connected inputs, which a list is opened with. */
  readonly listed: Asked<Fits>;
}

/** Asks which of the fields and inputs fit `slot`, while `open` and the slot is known. */
export function useCandidateFits(
  slot: FitSlot | null,
  open: boolean,
  frame: Frame,
  node: NodeView,
): CandidateFits {
  const listed = useMemo(() => listedCandidates(frame, node), [frame, node]);
  return { listed: useFitsOf(open ? slot : null, listed) };
}

/** What a text typed into a list is asked about: what it is typed into, and where it would stand. */
export interface TypedAsk {
  /** What is typed into the list's search box. */
  readonly text: string;
  /** What the list takes: an expression, or a literal of a kind. */
  readonly typed: TypedKind;
  /** Where the readings would be given, or `null` where it is only the text that is read. */
  readonly slot: FitSlot | null;
}

/**
 * Asks the engine how `ask.text` reads as what it is typed into, while it
 * is not `null` and something is typed. The engine says which literals the
 * text is and their dtypes, or why it is not what is wanted; nothing here
 * works that out.
 */
export function useReadTyped(ask: TypedAsk | null): Asked<readonly PreparedReading[]> {
  const request: TypedRequest | null =
    ask === null || ask.text === ''
      ? null
      : {
          text: ask.text,
          typed: ask.typed,
          start: ask.slot?.start ?? null,
          argument: ask.slot?.argument ?? null,
        };
  return useAsk<TypedRequest, readonly PreparedReading[]>(request, async (host, asked) =>
    prepareReadings(await host.readTyped(asked), asked.start !== null),
  );
}
