/**
 * @file Prepares what the engine reports of a run for display, and nothing else: for each step of
 * the document the columns it is given and the columns it gives, which of them it added, the dtype
 * after the start of each chain and after each method, whether the step stays lazy, and what is the
 * matter with it, and where. The report and the document come from the host together, so the places
 * in the report are places in the document the editor draws. Everything here is read off the report;
 * no rule about a dtype, a column or a value is worked out in the view.
 */

import type {
  CallView,
  CauseView,
  ColumnChangeView,
  CountView,
  DocumentStepView,
  ErrorKind,
  OfferPlaceView,
  PlaceView,
  ResolutionView,
  RowsView,
  RunView,
  StepReportView,
  StructView,
} from '@dageist/host';
import { HostError } from '@dageist/host';

import { HEAD } from './catalogue';
import type { Path } from './doc';
import { givenOf, pathKey } from './doc';
import type { Frame } from './frame';
import { findColumn } from './frame';
import type { Outcome } from './latest';
import { valuesLine } from './values';

/**
 * The dtype after the start of an expression and after each of its links, and where each link
 * stands; `null` where the engine gives none.
 */
export interface ChainDtypes {
  /** The dtype after the start. */
  readonly source: string | null;
  /** The dtype after each link, in order. */
  readonly links: ReadonlyArray<string | null>;
  /**
   * The place of each link of the expression, then the place a link added at the end would have, as
   * the engine's walk reports them: what each follows. `null` where the engine could not say. What
   * may stand at a place is asked of the engine at it.
   */
  readonly places: ReadonlyArray<OfferPlaceView | null>;
  /** The struct each of the places follows, as the engine writes it; `null` where it follows none. */
  readonly fields: ReadonlyArray<StructView | null>;
}

/** What a step is, as far as anyone knows. */
export type StepStatus =
  /** The engine accepts it. */
  | 'valid'
  /** The engine refused it. */
  | 'refused'
  /** It holds something still to be picked, so the engine walked it up to there and did not build it. */
  | 'incomplete'
  /** A step before it cannot be run, so it was not. */
  | 'notRun';

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
  /** What the step works on in a few words, as the engine wrote it. */
  readonly summary: string;
  /** The columns the step added or replaced, as the engine reports them. */
  readonly changes: readonly ColumnChangeView[];
  /** How many rows the step kept of those it received, or `null` where the engine did not count them. */
  readonly rows: RowsKept | null;
  /** The first values of each column the step changed, by the column's name, each as one line of text. */
  readonly first: ReadonlyMap<string, string>;
  /** Whether the step stays in a lazy plan. */
  readonly lazy: boolean;
  /**
   * Why the engine could not run the step on the sample rows, in words, though it accepts the step:
   * it failed there, or a step before it did. `null` where there is none.
   */
  readonly rowsProblem: string | null;
  /** What is the matter with the step, in words, or `null`. */
  readonly problem: string | null;
  /** Where in the step the matter is, as a way from the step, or `null` where it is the whole step. */
  readonly problemAt: Path | null;
  /** The dtypes of each expression of the step, by the key of the way to it. */
  readonly chains: ReadonlyMap<string, ChainDtypes>;
  /**
   * The ways to the arguments of the step and of the calls inside it that are set to what Polars
   * holds when they are not given, as the engine says, by key.
   */
  readonly atDefault: ReadonlySet<string>;
}

/** What the editor shows of a run. */
export interface PreparedRun {
  /** The columns of the table the steps start from. */
  readonly source: Frame;
  /** One for each step of the document, in its order. */
  readonly steps: readonly PreparedStep[];
}

/** Returns the dtype a resolution gives, or `null` when it gives none. */
function dtypeOf(resolution: ResolutionView | undefined): string | null {
  return resolution?.kind === 'dtype' ? resolution.dtype : null;
}

/** Returns the way a place names, from its step. */
function wayOf(place: PlaceView): Path {
  return place.path;
}

/**
 * Returns what the engine reports of each expression of a step by the key of its way: the dtypes
 * built, and the places its walk reported for the links, which a step it could not build, as one
 * with a hole, still has.
 */
function chainsOf(report: StepReportView): ReadonlyMap<string, ChainDtypes> {
  const found = new Map<string, ChainDtypes>();
  for (const expression of report.expressions) {
    found.set(pathKey(wayOf(expression.place)), {
      source: dtypeOf(expression.start),
      links: expression.links.map(dtypeOf),
      places: [],
      fields: [],
    });
  }
  // A join's `on` is walked twice, as a left key and as a right key, so the walk holds two
  // expressions at its place, the left then the right; the first is read.
  const walked = new Set<string>();
  for (const expression of report.walk.expressions) {
    const key = pathKey(wayOf(expression.place));
    if (walked.has(key)) continue;
    walked.add(key);
    const held = found.get(key);
    found.set(key, {
      source: held?.source ?? null,
      links: held?.links ?? [],
      places: expression.places,
      fields: expression.fields,
    });
  }
  return found;
}

/**
 * Returns what the hint of the method at `index` of a chain says of what it is handed: the dtype in
 * and the dtype out where the engine built them, and otherwise, for a step the walk refused, which
 * is not planned, the class of the value the walk says the method follows. `null` where the engine
 * gives neither.
 */
export function linkHint(dtypes: ChainDtypes | undefined, index: number): string | null {
  if (dtypes === undefined) return null;
  const input = index === 0 ? dtypes.source : (dtypes.links[index - 1] ?? null);
  if (input !== null) return `${input} → ${dtypes.links[index] ?? 'does not fit'}`;
  const after = dtypes.places[index]?.after;
  if (after?.kind !== 'value') return null;
  // The tables measure one struct of two fields, so the dtype of a struct is the one the engine
  // writes for what the walk carries, and not its class.
  return `after a value of the class ${dtypes.fields[index]?.dtype ?? after.dtype}`;
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

/** Returns how a step is called in words: its number in the list `steps`, or `a step before it` where the list lacks it. */
function stepWords(steps: readonly DocumentStepView[], id: string): string {
  const at = steps.findIndex((step) => step.id === id);
  return at < 0 ? 'a step before it' : `step ${String(at + 1)}`;
}

/**
 * Returns how many rows a step kept, and the first values of what it changed, from how the engine
 * counted it. `before` are the steps ahead of it, which name the step that failed where the step
 * was not reached.
 */
function countOf(
  count: CountView,
  before: readonly DocumentStepView[],
): Pick<PreparedStep, 'rows' | 'first' | 'rowsProblem'> {
  switch (count.kind) {
    case 'notCounted':
      return { rows: null, first: new Map(), rowsProblem: null };
    case 'notReached':
      return {
        rows: null,
        first: new Map(),
        rowsProblem: `Not run on the sample rows: ${stepWords(before, count.failed)} failed on them.`,
      };
    case 'counted':
      return {
        rows: { received: count.received, kept: count.kept },
        first: new Map(count.first.map((column) => [column.name, valuesLine(column.values)])),
        rowsProblem: null,
      };
    case 'failed':
      return {
        rows: null,
        first: new Map(),
        rowsProblem: `Could not be run on the sample rows: ${firstParagraph(count.reason)}`,
      };
    default:
      return count satisfies never;
  }
}

/** What is said of a step that has to wait for the step called `blockedBy`. */
function waitingWords(
  steps: readonly DocumentStepView[],
  blockedBy: string,
  incomplete: boolean,
): string {
  const which = stepWords(steps, blockedBy);
  return incomplete
    ? `Not run: ${which} still has something to be picked.`
    : `Not run: ${which} was refused.`;
}

/** What is said of a step with something still to be picked. */
const STILL_TO_PICK = 'Something in this step is still to be picked.';

/**
 * Prepares what the engine reported of the document whose steps are `steps`, for display. The
 * report is the host's of the very document, so each step's report is found by its place in the
 * list.
 */
export function prepareRun(steps: readonly DocumentStepView[], run: RunView): PreparedRun {
  const source: Frame = run.source;
  let before = source;
  const incomplete = new Set<string>();
  const prepared: PreparedStep[] = [];
  for (const [index, report] of run.steps.entries()) {
    const base = {
      id: report.id,
      before,
      summary: report.summary,
      lazy: report.lazy,
      chains: chainsOf(report),
      atDefault: new Set(report.atDefault.map((place) => pathKey(wayOf(place)))),
      ...countOf(report.count, steps.slice(0, index)),
    };
    const { outcome } = report;
    let result: PreparedStep;
    switch (outcome.status) {
      case 'valid':
        result = {
          ...base,
          status: 'valid',
          after: outcome.columns,
          changes: outcome.changes,
          problem: null,
          problemAt: null,
        };
        break;
      case 'refused':
        result = {
          ...base,
          status: 'refused',
          after: before,
          changes: [],
          problem: causeWords(outcome.cause),
          problemAt: wayOf(outcome.place),
        };
        break;
      case 'incomplete':
        incomplete.add(report.id);
        result = {
          ...base,
          status: 'incomplete',
          after: before,
          changes: [],
          problem: STILL_TO_PICK,
          problemAt: wayOf(outcome.place),
        };
        break;
      case 'notRun':
        result = {
          ...base,
          status: 'notRun',
          after: before,
          changes: [],
          problem: waitingWords(
            steps.slice(0, index),
            outcome.blockedBy,
            incomplete.has(outcome.blockedBy),
          ),
          problemAt: null,
        };
        break;
      default:
        return outcome satisfies never;
    }
    prepared.push(result);
    before = result.after;
  }
  return { source, steps: prepared };
}

/** Returns whether the step changed the column `name`: it added it or replaced it, as the engine reports. */
export function isChanged(changes: readonly ColumnChangeView[], name: string): boolean {
  return changes.some((change) => change.name === name);
}

/** What the editor says of a step beside its row. */
export type Saying =
  /** The engine ran the step on the rows and counted them, so the rows say what it did. */
  | { readonly says: 'rows' }
  /** Something is the matter, said in these lines. */
  | { readonly says: 'problem'; readonly lines: readonly string[] }
  /** The step was not counted and nothing is said to be the matter: the row says nothing of it. */
  | { readonly says: 'nothing' };

/**
 * Returns what the editor says of `step` beside its row: the matter with it and what went wrong
 * running it on the sample rows, one line each, or that its rows were counted, or nothing.
 */
export function sayingOf(step: PreparedStep): Saying {
  const lines = [step.problem, step.rowsProblem].flatMap((line) => (line === null ? [] : [line]));
  if (lines.length > 0) return { says: 'problem', lines };
  return step.rows === null ? { says: 'nothing' } : { says: 'rows' };
}

/** One column a join added, with what the engine said of it. */
export interface AddedColumn {
  readonly name: string;
  readonly dtype: string;
  /** The first values of the column, as one line, or `null` where the engine ran no rows. */
  readonly first: string | null;
}

/** What stands at the end of a step's row. */
export type RowEnd =
  /** A pivot: how many columns it returns, and which are new. */
  | { readonly kind: 'pivot'; readonly columns: number; readonly added: readonly string[] }
  /** An added column: its name, its dtype and its first values. */
  | {
      readonly kind: 'column';
      readonly alias: string;
      readonly dtype: string;
      readonly first: string | null;
    }
  /** A join: the columns it adds. */
  | { readonly kind: 'join'; readonly added: readonly AddedColumn[] }
  /** A filter: how many rows it kept of those it was given. */
  | { readonly kind: 'kept'; readonly kept: number; readonly received: number };

/** Returns the name of the column an `Add column` step adds. */
function addedAlias(call: CallView): string | null {
  const named = givenOf(call, HEAD.with_columns.named);
  const key = named?.kind === 'pairs' ? named.entries[0]?.key : undefined;
  return key?.kind === 'columns' ? (key.names[0] ?? null) : null;
}

/**
 * Returns what stands at the end of the row of the step `call`, which `result` reports, or `null`
 * where nothing does: the step was not accepted, or it is one that says nothing there.
 */
export function rowEndOf(call: CallView, result: PreparedStep): RowEnd | null {
  if (result.status !== 'valid') return null;
  const added = result.changes
    .filter((change) => change.change === 'added')
    .map((change) => change.name);
  switch (call.entry) {
    case 'LazyFrame.pivot':
      return { kind: 'pivot', columns: result.after.length, added };
    case 'LazyFrame.with_columns': {
      const alias = addedAlias(call);
      const dtype = alias === null ? undefined : findColumn(result.after, alias)?.dtype;
      if (alias === null || dtype === undefined) return null;
      return { kind: 'column', alias, dtype, first: result.first.get(alias) ?? null };
    }
    case 'LazyFrame.join':
      return {
        kind: 'join',
        added: added.map((name) => ({
          name,
          dtype: findColumn(result.after, name)?.dtype ?? '',
          first: result.first.get(name) ?? null,
        })),
      };
    case 'LazyFrame.filter':
      return result.rows === null
        ? null
        : { kind: 'kept', kept: result.rows.kept, received: result.rows.received };
    default:
      return null;
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
    case 'stale':
      return 'The steps changed; this will be asked again.';
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
