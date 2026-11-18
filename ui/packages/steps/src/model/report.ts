/**
 * @file Prepares what the engine reports of a run for display, and nothing
 * else: for each step of the editor the columns it is given and the columns
 * it gives, which of them it added, the dtype after the start of each chain
 * and after each method, whether the step stays lazy, and what is the matter
 * with it, on which word. Everything here is read off the report or off what
 * the translation noted; no rule about a dtype, a column or a value is worked
 * out in the view.
 */

import type {
  CauseView,
  ColumnChangeView,
  CountView,
  ErrorKind,
  OfferPlaceView,
  PlaceView,
  ResolutionView,
  RowsView,
  RunView,
  StepReportView,
} from '@dageist/host';
import { HostError } from '@dageist/host';

import type { Mark, SentStep, Translation } from './document';
import { placeKey, segmentWord, wrongId } from './document';
import type { Frame } from './frame';
import type { Outcome } from './latest';
import { isRecord } from './shape';
import type { Step } from './steps';
import { valuesLine } from './values';

/**
 * The dtype after the start of a chain and after each of its methods, and
 * where each method stands; `null` where the engine gives none.
 */
export interface ChainDtypes {
  readonly source: string | null;
  readonly methods: ReadonlyArray<string | null>;
  /**
   * The place of each method of the chain, then the place a method added at the
   * end would have, as the engine's walk reports them: what each follows. `null`
   * where the engine could not say. What may stand at a place is asked of the
   * engine at it.
   */
  readonly places: ReadonlyArray<OfferPlaceView | null>;
}

/** What a step of the editor is, as far as anyone knows. */
export type StepStatus =
  /** The engine accepts it. */
  | 'valid'
  /** It cannot be run: the engine refused it, or it cannot be written as a document yet. */
  | 'refused'
  /** A step before it cannot be run, so it was not. */
  | 'notRun'
  /** It is in the document, and the engine has not reported on it. */
  | 'unknown';

/** How many rows a step kept of those it was given, on the working window. */
export interface RowsKept {
  readonly received: number;
  readonly kept: number;
}

/** What the editor shows of one step. */
export interface PreparedStep {
  readonly id: string;
  readonly status: StepStatus;
  /** The columns the step is given. */
  readonly before: Frame;
  /** The columns the step gives; those it is given when it cannot be run. */
  readonly after: Frame;
  /** The columns the step added or replaced, as the engine reports them. */
  readonly changes: readonly ColumnChangeView[];
  /** How many rows the step kept of those it received, or `null` where the engine did not count them. */
  readonly rows: RowsKept | null;
  /** The first values of each column the step changed, by the column's name, each as one line of text. */
  readonly first: ReadonlyMap<string, string>;
  /** Whether the step stays in a lazy plan, and `null` where the engine has not said. */
  readonly lazy: boolean | null;
  /** Why the engine could not run the step on the sample rows, in words, though it accepts the step, or `null`. */
  readonly rowsProblem: string | null;
  /** What is the matter with the step, in words, or `null`. */
  readonly problem: string | null;
  /** The words of the editor that are the matter, as `wrongId` names them. */
  readonly wrong: ReadonlySet<string>;
  /** The dtypes of each chain of the step, by the chain's identifier. */
  readonly chains: ReadonlyMap<string, ChainDtypes>;
}

/** What the editor shows of a run. */
export interface PreparedRun {
  /** The columns of the table the steps start from. */
  readonly source: Frame;
  /** One for each step of the editor, in its order. */
  readonly steps: readonly PreparedStep[];
}

const NO_WORDS: ReadonlySet<string> = new Set();
const NO_CHAINS: ReadonlyMap<string, ChainDtypes> = new Map();
const NO_FIRST: ReadonlyMap<string, string> = new Map();

/** Returns the dtype a resolution gives, or `null` when it gives none. */
function dtypeOf(resolution: ResolutionView | undefined): string | null {
  return resolution?.kind === 'dtype' ? resolution.dtype : null;
}

/**
 * Returns how the engine's expressions fall on the chains of the editor: the dtypes it built them
 * to, where it built them, and the places its walk reported for their links, which a step it
 * could not build, as one with a hole, still has.
 */
function chainDtypes(report: StepReportView, sent: SentStep): ReadonlyMap<string, ChainDtypes> {
  const found = new Map<string, ChainDtypes>();
  for (const [key, owner] of sent.chains) {
    const expression = report.expressions.find(
      (candidate) => placeKey(candidate.place.path.map(segmentWord)) === key,
    );
    // A join's `on` is walked twice, as a left key and as a right key, so the walk holds two
    // expressions at its place, the left then the right; `find` reads the left.
    const walked = report.walk.expressions.find(
      (candidate) => placeKey(candidate.place.path.map(segmentWord)) === key,
    );
    if (expression === undefined && walked === undefined) continue;
    const { whenLinks, methodIds } = owner;
    // A chain that starts from `when` has a dtype once its `otherwise` is there.
    const source = whenLinks === 0 ? expression?.start : expression?.links[whenLinks - 1];
    // The walk says where each link stands, from the first after the start; the methods are
    // those after the links of a condition, and one more place follows them.
    const places = walked?.places.slice(whenLinks) ?? [];
    found.set(owner.chainId, {
      source: dtypeOf(source),
      methods: methodIds.map((_id, index) => dtypeOf(expression?.links[whenLinks + index])),
      places: Array.from({ length: methodIds.length + 1 }, (_none, index) => places[index] ?? null),
    });
  }
  return found;
}

/** Returns the word of the editor a place of a step is, or `null` when no word is it. */
function markAt(sent: SentStep, place: PlaceView): Mark | null {
  const words = place.path.map(segmentWord);
  // The longest place that stands for a word: a place deeper than one is in that word.
  for (let length = words.length; length >= 0; length -= 1) {
    const found = sent.marks.get(placeKey(words.slice(0, length)));
    if (found !== undefined) return found;
  }
  return null;
}

/** Returns the first paragraph of `reason`, which is what Polars says before it prints its plan. */
function firstParagraph(reason: string): string {
  return (reason.split('\n\n')[0] ?? reason).trim();
}

/** Returns what a cause says, in words. */
function causeWords(cause: CauseView): string {
  switch (cause.kind) {
    case 'unknownTable':
      return `There is no table "${cause.name}".`;
    case 'unknownInput':
      return `There is no input "${cause.name}".`;
    case 'build':
      return firstParagraph(cause.reason);
    case 'notAllowed':
      return cause.refusal.reason;
    case 'polars':
      return firstParagraph(cause.reason);
    default:
      return cause satisfies never;
  }
}

/** Returns how many rows a step kept, and the first values of what it changed, from how the engine counted it. */
function countOf(count: CountView): Pick<PreparedStep, 'rows' | 'first' | 'rowsProblem'> {
  switch (count.kind) {
    case 'notCounted':
      return { rows: null, first: NO_FIRST, rowsProblem: null };
    case 'counted':
      return {
        rows: { received: count.received, kept: count.kept },
        first: new Map(count.first.map((column) => [column.name, valuesLine(column.values)])),
        rowsProblem: null,
      };
    case 'failed':
      return {
        rows: null,
        first: NO_FIRST,
        rowsProblem: `Could not be run on the sample rows: ${firstParagraph(count.reason)}`,
      };
    default:
      return count satisfies never;
  }
}

/** What is said of a step that has to wait for the step called `blockedBy`. */
function waitingWords(steps: readonly Step[], blockedBy: string): string {
  const at = steps.findIndex((step) => step.id === blockedBy);
  return at < 0
    ? 'Not run: a step before it cannot be run.'
    : `Not run: step ${String(at + 1)} cannot be run yet.`;
}

/** What the engine reported, and the document it was asked about. */
export interface Answered {
  /** The document's JSON text. */
  readonly document: string;
  readonly run: RunView;
}

/** Returns a step's report: what the engine said of it, found by the step's identifier. */
function reportsById(run: RunView | null): ReadonlyMap<string, StepReportView> {
  return run?.status === 'run'
    ? new Map(run.steps.map((report) => [report.id, report]))
    : new Map();
}

/** Returns each step of the document `text` as the JSON text `JSON.stringify` writes it. */
function stepTexts(text: string): readonly string[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    // A text that is not JSON has no steps to compare with.
    return [];
  }
  return isRecord(parsed) && Array.isArray(parsed.steps)
    ? parsed.steps.map((step: unknown) => JSON.stringify(step))
    : [];
}

/**
 * Prepares what the engine reported of the document `answered` for the editor's
 * `steps`, for display. `translation` is the document the steps are now: a
 * report is shown only for the steps that are still what it reported of. A step
 * that an edit has changed, and every step after it, whose columns may differ
 * now, is shown as waiting for the engine, with the columns the last step that
 * still corresponds gives. `answered` is `null` where the engine has not
 * reported; `source` is the columns of the table the steps start from.
 *
 * A text the engine could not read is not a report of steps. The first step
 * in the document is then the one refused, with the engine's reason, and the
 * steps after it are not run.
 */
export function prepareRun(
  steps: readonly Step[],
  translation: Translation,
  answered: Answered | null,
  source: Frame,
): PreparedRun {
  const run = answered?.run ?? null;
  const reports = reportsById(run);
  const unreadable =
    run?.status === 'unreadable' && answered?.document === translation.text ? run.problem : null;
  const answeredTexts = answered === null ? [] : stepTexts(answered.document);
  let corresponds = true;
  let sentSoFar = 0;
  // The steps the engine said are incomplete, which the steps after them wait for.
  const incomplete = new Set<string>();
  let before = source;
  const prepared: PreparedStep[] = [];
  for (const [index, step] of steps.entries()) {
    const found = translation.steps[index];
    const make = (
      status: StepStatus,
      problem: string | null,
      rest: Partial<PreparedStep> = {},
    ): PreparedStep => ({
      id: step.id,
      status,
      before,
      after: before,
      changes: [],
      rows: null,
      first: NO_FIRST,
      rowsProblem: null,
      lazy: null,
      problem,
      wrong: NO_WORDS,
      chains: NO_CHAINS,
      ...rest,
    });
    let result: PreparedStep;
    if (found === undefined) {
      result = make('unknown', null);
    } else if (found.kind === 'held') {
      result = make('refused', found.problem.message, { wrong: new Set(found.problem.wrong) });
    } else if (found.kind === 'waiting') {
      result = make('notRun', waitingWords(steps, found.blockedBy));
    } else if (unreadable !== null) {
      // The reader names the step it stopped at; with no step, the first of the document.
      const stoppedAt = unreadable.kind === 'refused' ? (unreadable.place.stepIndex ?? 0) : 0;
      if (sentSoFar < stoppedAt) {
        result = make('unknown', null);
      } else if (sentSoFar === stoppedAt) {
        result = make('refused', unreadable.reason);
      } else {
        result = make('notRun', 'Not run: a step before it cannot be read.');
      }
    } else {
      // What was reported is of this step only while it and every step before it are as they were.
      corresponds = corresponds && answeredTexts[sentSoFar] === found.canonical;
      const report = corresponds ? reports.get(step.id) : undefined;
      result =
        report === undefined
          ? make('unknown', null)
          : fromReport(step, found, report, before, steps, incomplete);
      if (report?.outcome.status === 'incomplete') incomplete.add(step.id);
    }
    if (found?.kind === 'sent') sentSoFar += 1;
    prepared.push(result);
    before = result.after;
  }
  return { source, steps: prepared };
}

/** Prepares one step from what the engine reported of it. */
function fromReport(
  step: Step,
  sent: SentStep,
  report: StepReportView,
  before: Frame,
  steps: readonly Step[],
  incomplete: ReadonlySet<string>,
): PreparedStep {
  const base = {
    id: step.id,
    before,
    lazy: report.lazy,
    chains: chainDtypes(report, sent),
    ...countOf(report.count),
  };
  const { outcome } = report;
  switch (outcome.status) {
    case 'valid':
      return {
        ...base,
        status: 'valid',
        after: outcome.columns,
        changes: outcome.changes,
        problem: null,
        wrong: NO_WORDS,
      };
    case 'refused': {
      const found = markAt(sent, outcome.place);
      return {
        ...base,
        status: 'refused',
        after: before,
        changes: [],
        problem: causeWords(outcome.cause),
        wrong: found === null ? NO_WORDS : new Set([wrongId(found)]),
      };
    }
    case 'incomplete': {
      // The place of the hole says what the editor has not got yet, and on which word.
      const hole = sent.holes.get(placeKey(outcome.place.path.map(segmentWord)));
      return {
        ...base,
        status: 'refused',
        after: before,
        changes: [],
        problem: hole?.message ?? 'Something in this step is still to be picked.',
        wrong: hole === undefined ? NO_WORDS : new Set(hole.wrong),
      };
    }
    case 'notRun':
      return {
        ...base,
        status: 'notRun',
        after: before,
        changes: [],
        problem: incomplete.has(outcome.blockedBy)
          ? waitingWords(steps, outcome.blockedBy)
          : 'Not run: a step before it was refused.',
        wrong: NO_WORDS,
      };
    default:
      return outcome satisfies never;
  }
}

/** What the rows of a step show: the rows, or why there are none. */
export type RowsDisplay =
  | {
      readonly kind: 'rows';
      readonly columns: Frame;
      readonly rows: ReadonlyArray<ReadonlyArray<string | null>>;
    }
  | { readonly kind: 'message'; readonly text: string };

/**
 * Says why the engine gave no rows for a step, by the kind of the failure and
 * never by its text, except where the text is all there is to say.
 * `step` is what is known of the step asked for, when it is known.
 */
export function rowsFailure(kind: ErrorKind, message: string, step: PreparedStep | null): string {
  switch (kind) {
    case 'noSuchStep':
      return 'This step is not in the document yet.';
    case 'notRun':
      return step?.problem ?? 'Not run: a step before it was refused.';
    case 'refused':
      return step?.problem ?? 'This step was refused.';
    case 'limit':
      return 'There are more rows than a preview holds.';
    case 'unreadable':
      return 'The steps cannot be read yet.';
    case 'unknownName':
    case 'failed':
      return message;
    default:
      return kind satisfies never;
  }
}

/** What the host answered when asked for the rows after a step. */
export interface RowsAnswer {
  /** The step the rows were asked for. */
  readonly step: string;
  readonly outcome: Outcome<RowsView>;
}

/**
 * Returns what to show of the rows after the step `selectedId`: the rows, or
 * why there are none, and `null` while there is nothing to show yet. An answer
 * for another step is not shown, rows or failure, so choosing a step never
 * shows what was said of the one before it. `step` is what is known of the
 * chosen step.
 */
export function rowsToShow(
  answer: RowsAnswer | null,
  selectedId: string,
  step: PreparedStep | null,
): RowsDisplay | null {
  if (answer === null || answer.step !== selectedId) return null;
  const { outcome } = answer;
  if (outcome.ok) {
    return { kind: 'rows', columns: outcome.answer.columns, rows: outcome.answer.rows };
  }
  const { error } = outcome;
  return {
    kind: 'message',
    text:
      error instanceof HostError
        ? rowsFailure(error.kind, error.message, step)
        : error instanceof Error
          ? error.message
          : String(error),
  };
}
