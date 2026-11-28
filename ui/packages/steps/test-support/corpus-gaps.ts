/**
 * @file Test support: rebuilds every document of the corpus the host wrote from what the model
 * offers, and returns the pieces it cannot build, which a test compares with the declared gaps. A
 * document is made of pieces: where each chain starts, each method after it, each argument and
 * option, and each step. For each piece this asks the functions the components call whether the
 * editor has an action that makes it: the start from the list at its place, a method by adding
 * after the piece before it, an argument or an option from the control its parameter gets, a step
 * from the list of steps; and whether the step says something. It is not part of the package's
 * front door.
 */

import type {
  ArgumentView,
  CallView,
  EntryView,
  InputView,
  OfferPlaceView,
  ParameterView,
  StartView,
  StepReportView,
  VerdictView,
  WalkedExpressionView,
} from '@dageist/host';

import type { CorpusDocument } from '../../host/test-support/corpus';
import { CONDITION_TESTS, HEAD, stepOptionParameters, verbOf } from '../src/model/catalogue';
import type { Catalogue } from '../src/model/catalogue';
import type { OneControl, ParameterControl } from '../src/model/controls';
import { controlOf, settingOf } from '../src/model/controls';
import type { ExpressionView, Path } from '../src/model/doc';
import {
  START_SEGMENT,
  fieldNameOf,
  itemSegment,
  keySegment,
  linkSegment,
  parameterSegment,
  pathKey,
  under,
  valueSegment,
  whenOf,
} from '../src/model/doc';
import type { Cover, Gap, StartKind } from '../src/model/limits';
import { FOUND_BY, coverKey } from '../src/model/limits';
import { methodFamilies, startOffers } from '../src/model/offers';
import type { PreparedOffer } from '../src/model/offers';
import { START } from '../src/model/place';
import { prepareRun, sayingOf } from '../src/model/report';
import type { PreparedStep } from '../src/model/report';
import { OFFERED_INPUTS, actionsOf, lineDrawing, listsOf, valueDrawing } from '../src/model/words';
import { undrawnKind } from './grid-gaps';
import { actionsOfSlot, canPick, isDrawn, routeOf, wordsAt } from './places';
import { sorted } from './sorted';

/** What a rebuild of the corpus found. */
export interface CorpusFound {
  /** The gaps found, by cover key, each with the pieces that showed it. */
  readonly gaps: ReadonlyMap<string, { readonly cover: Cover; readonly pieces: string[] }>;
  /** What is wrong that no gap can declare: a piece the model should offer an action for and a kind that disagrees with the control. */
  readonly failures: readonly string[];
  /** How many pieces were rebuilt. */
  readonly pieces: number;
  /** How many chains the walk did not place, so that no list at their place could be asked. */
  readonly unplaced: readonly string[];
}

/** Returns whether `value` is one value of the kind `control` draws, or why it is not. */
function oneFits(control: OneControl, value: ArgumentView): string | null {
  const expected = ((): ReadonlyArray<ArgumentView['kind']> => {
    switch (control.control) {
      case 'expression':
        return ['expression'];
      case 'column':
        return ['columns'];
      case 'field':
        return ['field'];
      case 'literal':
        switch (control.type) {
          case 'integer':
            return ['integer'];
          case 'float':
            return ['float', 'integer'];
          case 'string':
          case 'date':
            return ['string'];
          case 'any':
            return ['integer', 'float', 'string', 'flag'];
          case 'duration':
            return ['duration'];
          case 'pattern':
            return ['pattern'];
          case 'timeZone':
            return ['timeZone'];
          case 'path':
            return ['path'];
          default:
            return control.type satisfies never;
        }
      case 'flag':
        return ['flag'];
      case 'choice':
        return ['choice'];
      case 'dtype':
        return ['dtype'];
      case 'timeZone':
        return ['timeZone'];
      case 'table':
        return ['table'];
      case 'none':
        return [];
      default:
        return control satisfies never;
    }
  })();
  return expected.includes(value.kind)
    ? null
    : `a ${value.kind}, where ${control.control} is drawn`;
}

/** Returns whether `value` is held as `control` holds a parameter, or why it is not. */
export function argumentFits(control: ParameterControl, value: ArgumentView): string | null {
  switch (control.shape) {
    case 'one':
      return oneFits(control.one, value);
    case 'columns':
      if (value.kind === 'columns') return null;
      return value.kind === 'many' && value.items.every((item) => item.kind === 'columns')
        ? null
        : `a ${value.kind}, where fields are drawn`;
    case 'many':
      if (value.kind !== 'many') return `a ${value.kind}, where a list is drawn`;
      for (const item of value.items) {
        const problem = oneFits(control.each, item);
        if (problem !== null) return `an item that is ${problem}`;
      }
      return null;
    case 'pairs': {
      if (value.kind !== 'pairs') return `a ${value.kind}, where entries are drawn`;
      for (const entry of value.entries) {
        const problem = oneFits(control.key, entry.key) ?? oneFits(control.value, entry.value);
        if (problem !== null) return `an entry that is ${problem}`;
      }
      return null;
    }
    case 'perItem':
      // One for each item of another parameter, or one for all of them.
      if (value.kind === 'many') {
        return value.items.every((item) => item.kind === 'flag' || item.kind === 'choice')
          ? null
          : 'an item that is no flag or choice';
      }
      return value.kind === 'flag' || value.kind === 'choice'
        ? null
        : `a ${value.kind}, where a flag or a choice for each item is drawn`;
    default:
      return control satisfies never;
  }
}

/** Returns the kind of start `start` is, which says whether the editor can add a method after it. */
function startKindOf(start: StartView, inputs: readonly InputView[]): StartKind | null {
  switch (start.kind) {
    case 'hole':
      return null;
    case 'input': {
      const input = inputs.find((held) => held.name === start.name);
      if (input?.kind === 'value') return 'connectedValue';
      return input?.kind === 'list' ? 'connectedList' : null;
    }
    case 'call': {
      if (fieldNameOf(start.call) !== null) return 'field';
      if (start.call.entry === 'polars.lit') {
        switch (start.call.arguments[0]?.value.kind) {
          case 'integer':
            return 'typedInteger';
          case 'float':
            return 'typedFloat';
          case 'string':
            return 'typedText';
          case 'flag':
            return 'typedFlag';
          default:
            return 'function';
        }
      }
      return 'function';
    }
    default:
      return start satisfies never;
  }
}

/** The engine's answer for one entry at a place, as a list is built from it. */
function offersFor(place: OfferPlaceView, entry: string, verdict: VerdictView) {
  return {
    place,
    entries: [{ entry, verdict, argument: null, leftOut: null, slots: [] }],
  };
}

/** The verdict a piece is given where the walk was not asked: it stands. */
const ALLOWED: VerdictView = {
  kind: 'allowed',
  result: null,
  shape: 'one',
  ending: { kind: 'allowed' },
};

/** Rebuilds the documents of the corpus. */
class Rebuild {
  readonly gaps = new Map<string, { cover: Cover; pieces: string[] }>();
  readonly failures: string[] = [];
  readonly unplaced: string[] = [];
  pieces = 0;
  /** The entry of the step being rebuilt. */
  private stepEntry = '';

  constructor(
    private readonly using: Catalogue,
    private readonly inputs: readonly InputView[],
  ) {}

  /** Notes a gap, with the piece that showed it. */
  private gap(cover: Cover, piece: string): void {
    const key = coverKey(cover);
    const held = this.gaps.get(key);
    if (held === undefined) this.gaps.set(key, { cover, pieces: [piece] });
    else held.pieces.push(piece);
  }

  /** Notes a failure that no gap can declare. */
  private fail(piece: string, why: string): void {
    this.failures.push(`${piece}: ${why}`);
  }

  /** Rebuilds `document`. */
  document(held: CorpusDocument): void {
    const { steps } = held.document;
    const prepared = prepareRun(steps, held.run);
    for (const [index, step] of steps.entries()) {
      const report = held.run.steps[index];
      const result = prepared.steps[index];
      if (report === undefined || result === undefined) {
        this.fail(held.name, `no report of step ${step.id}`);
        continue;
      }
      this.step(held.name, step.call, result, report);
    }
  }

  /** What the editor says of a step, and the step's own piece and parameters. */
  private step(name: string, call: CallView, result: PreparedStep, report: StepReportView): void {
    const piece = `${name} › step ${report.id}`;
    this.stepEntry = call.entry;
    this.pieces += 1;
    if (sayingOf(result).says === 'nothing') {
      this.fail(piece, 'the editor says nothing of the step');
    }
    const entry = this.using.entries.get(call.entry);
    if (entry === undefined) {
      this.fail(piece, `the catalogue has no ${call.entry}`);
      return;
    }
    const verb = verbOf(call.entry);
    const route = routeOf(entry);
    if (
      verb === null ||
      route.route !== 'group' ||
      !listsOf({ word: 'newStep' }).includes(route.group)
    ) {
      // A step no list adds is drawn by its parameters alone, which no list sets.
      this.gap({ kind: 'step', entry: call.entry }, piece);
      return;
    }
    const head = Object.values<string>(HEAD[verb]);
    const options = new Set(stepOptionParameters(this.using, call.entry).map((held) => held.name));
    for (const given of call.arguments) {
      const parameter = entry.parameters.find((held) => held.name === given.parameter);
      const at = `${piece} › ${given.parameter}`;
      if (parameter === undefined) {
        this.fail(at, 'the catalogue gives the step no such parameter');
        continue;
      }
      if (!head.includes(given.parameter) && !options.has(given.parameter)) {
        this.gap({ kind: 'stepParameter', entry: call.entry, parameter: given.parameter }, at);
        continue;
      }
      const line = verb === 'filter' && given.parameter === CONDITION_TESTS;
      this.argument(
        at,
        parameter,
        given.value,
        under([], parameterSegment(given.parameter)),
        report,
        line,
      );
    }
  }

  /** An argument that fills a parameter the editor draws with a control: the kind it holds and the expressions inside it. */
  private argument(
    piece: string,
    parameter: ParameterView,
    value: ArgumentView,
    path: Path,
    report: StepReportView,
    line = false,
  ): void {
    this.pieces += 1;
    const control = controlOf(parameter);
    if (!isDrawn(control)) {
      this.gap({ kind: 'parameterKind', of: undrawnKind(parameter) }, piece);
      return;
    }
    const problem = argumentFits(control, value);
    if (problem !== null) this.fail(piece, `the control does not draw ${problem}`);
    this.inside(piece, value, path, report, line);
  }

  /**
   * The expressions inside an argument, in the list or the pairs that hold them. `line` says that
   * they are the tests of a condition, which a line draws as a field, a comparison and a value.
   */
  private inside(
    piece: string,
    value: ArgumentView,
    path: Path,
    report: StepReportView,
    line = false,
  ): void {
    switch (value.kind) {
      case 'expression':
        this.expression(piece, value, path, report, line);
        return;
      case 'many':
        for (const [at, item] of value.items.entries()) {
          this.inside(`${piece}[${String(at)}]`, item, under(path, itemSegment(at)), report, line);
        }
        return;
      case 'pairs':
        for (const [at, entry] of value.entries.entries()) {
          const where = `${piece}[${String(at)}]`;
          this.inside(`${where} key`, entry.key, under(path, keySegment(at)), report);
          this.inside(`${where} value`, entry.value, under(path, valueSegment(at)), report);
        }
        return;
      case 'columns':
      case 'field':
      case 'integer':
      case 'float':
      case 'string':
      case 'flag':
      case 'choice':
      case 'dtype':
      case 'duration':
      case 'pattern':
      case 'timeZone':
      case 'table':
      case 'path':
        return;
      default:
        value satisfies never;
    }
  }

  /** The walk of the expression at `path`, which is the first where a join key is walked twice. */
  private walked(report: StepReportView, path: Path): WalkedExpressionView | null {
    const key = pathKey(path);
    return report.walk.expressions.find((held) => pathKey(held.place.path) === key) ?? null;
  }

  /** Rebuilds an expression: where it starts, each method after the piece before it, and what each is given. */
  private expression(
    piece: string,
    expression: ExpressionView,
    path: Path,
    report: StepReportView,
    line = false,
  ): void {
    const walked = this.walked(report, path);
    const context = walked?.context ?? null;
    if (walked === null) {
      // The step was not run, so nothing of it was walked.
      this.unplaced.push(piece);
    } else if (context === null) {
      // The walk reached the expression and named no position that holds it.
      const first = path[0];
      const parameter = first?.kind === 'parameter' ? first.name : '';
      this.gap({ kind: 'noContext', entry: this.stepEntry, parameter }, piece);
    }
    // The model's own preparation of the report gives the place of each link.
    const places = walked?.places ?? [];
    const calls = walked?.calls ?? [];
    this.pieces += 1;

    // The start, from the list at the place where the expression starts.
    const startPlace: OfferPlaceView | null = context === null ? null : { context, after: START };
    const { start } = expression;
    switch (start.kind) {
      case 'hole':
        break;
      case 'input': {
        const input = this.inputs.find((held) => held.name === start.name);
        const offered = OFFERED_INPUTS.some((kind) => kind === input?.kind);
        const listed =
          startPlace === null ||
          wordsAt(this.using, startPlace.context, 'start').some((word) =>
            listsOf(word).includes('inputs'),
          );
        // An input that no list offers, a table, is one the engine refuses to start a chain from.
        const refused = report.outcome.status === 'refused' ? report.outcome.cause : null;
        const saidSo = refused?.kind === 'unknownInput' && refused.name === start.name;
        if ((!offered && !saidSo) || !listed) {
          this.fail(`${piece} › start`, `no list offers the input ${start.name}`);
        }
        break;
      }
      case 'call':
        // What a function that no list starts a chain from is given cannot be set either.
        if (this.startCall(`${piece} › start`, start.call, startPlace, calls[0]?.verdict ?? null)) {
          this.callArguments(`${piece} › start`, start.call, under(path, START_SEGMENT), report);
        }
        break;
      default:
        start satisfies never;
    }

    // A test of a condition is drawn as a line: a field, a comparison and what it is compared with.
    const drawing = line ? lineDrawing(expression, this.using.comparisons) : null;
    if (drawing?.drawn === 'condition') {
      this.pieces += 1;
      const [comparison] = expression.links;
      if (comparison === undefined || !this.using.comparisons.has(comparison.entry)) {
        this.fail(
          `${piece} › comparison`,
          'a line of a condition holds no comparison the engine lists',
        );
        return;
      }
      this.callArguments(`${piece} › comparison`, comparison, under(path, linkSegment(0)), report);
      return;
    }

    // Each method, by adding after the piece before it.
    const made = whenOf(expression);
    for (const [at, link] of expression.links.entries()) {
      const where = `${piece} › link ${String(at)} ${link.entry}`;
      this.pieces += 1;
      const comesWithWhen = made !== null && at < made.before;
      if (!comesWithWhen) this.addedAfterPrevious(where, expression, at);
      if (this.linkCall(where, link, places[at] ?? null, calls[at + 1]?.verdict ?? null)) {
        this.callArguments(where, link, under(path, linkSegment(at)), report);
      }
    }
  }

  /** Whether the editor can add a method after the piece that stands before the link `at`. */
  private addedAfterPrevious(piece: string, expression: ExpressionView, at: number): void {
    if (at > 0) {
      if (!actionsOf('method').add) this.fail(piece, 'no method can be added after a method');
      return;
    }
    // A start with nothing after it is drawn as the slot of a value draws it.
    const alone: ExpressionView = { ...expression, links: [] };
    if (!actionsOfSlot(valueDrawing(alone, true), false).add) {
      const kind = startKindOf(expression.start, this.inputs);
      if (kind === null) this.fail(piece, 'a method follows a start of no kind');
      else this.gap({ kind: 'start', start: kind }, piece);
    }
  }

  /** The start call: its entry is picked from the list at the start place. */
  private startCall(
    piece: string,
    call: CallView,
    place: OfferPlaceView | null,
    verdict: VerdictView | null,
  ): boolean {
    const entry = this.using.entries.get(call.entry);
    if (entry === undefined) {
      this.fail(piece, `the catalogue has no ${call.entry}`);
      return false;
    }
    if (place === null) return true;
    if (!canPick(this.using, entry, place)) {
      this.gap({ kind: 'entry', entry: call.entry }, piece);
      return false;
    }
    const rows = startOffers(this.using, offersFor(place, call.entry, verdict ?? ALLOWED));
    return this.rowIsRight(piece, entry, rows, verdict);
  }

  /** The link: its entry is picked from the list that adds a method at the place the engine reports. */
  private linkCall(
    piece: string,
    call: CallView,
    place: OfferPlaceView | null,
    verdict: VerdictView | null,
  ): boolean {
    const entry = this.using.entries.get(call.entry);
    if (entry === undefined) {
      this.fail(piece, `the catalogue has no ${call.entry}`);
      return false;
    }
    if (place === null) return true;
    if (!canPick(this.using, entry, place)) {
      this.gap({ kind: 'entry', entry: call.entry }, piece);
      return false;
    }
    const rows = methodFamilies(
      this.using,
      offersFor(place, call.entry, verdict ?? ALLOWED),
    ).flatMap((family) => family.offers);
    return this.rowIsRight(piece, entry, rows, verdict);
  }

  /**
   * Whether a list has a row for an entry that the engine's verdict decides the state of. An entry
   * with no row is one no list offers, which is a gap.
   */
  private rowIsRight(
    piece: string,
    entry: EntryView,
    rows: readonly PreparedOffer[],
    verdict: VerdictView | null,
  ): boolean {
    // A call picked with another, as `then` and `otherwise` are with `when`, has no row of its own,
    // and a field and a typed value are picked from the fields and the typed values, not from rows
    // the engine offers.
    const picked = routeOf(entry);
    if (picked.route !== 'group' || !['functions', 'methods'].includes(picked.group)) return true;
    const row = rows.find((held) => held.id === entry.id);
    if (row === undefined) {
      this.gap({ kind: 'entry', entry: entry.id }, piece);
      return false;
    }
    const refused = verdict?.kind === 'refused';
    if (refused !== (row.stoppedBy === 'engine')) {
      this.fail(piece, `the row of ${entry.id} is stopped by ${row.stoppedBy ?? 'no one'}`);
    }
    return true;
  }

  /** What a call is given: each argument, from the control its parameter gets. */
  private callArguments(piece: string, call: CallView, path: Path, report: StepReportView): void {
    const entry = this.using.entries.get(call.entry);
    if (entry === undefined) return;
    for (const given of call.arguments) {
      const parameter = entry.parameters.find((held) => held.name === given.parameter);
      const at = `${piece} › ${given.parameter}`;
      if (parameter === undefined) {
        this.fail(at, `the catalogue gives ${call.entry} no such parameter`);
        continue;
      }
      const setting = settingOf(entry, parameter);
      const inside = under(path, parameterSegment(given.parameter));
      switch (setting.by) {
        case 'entry':
          // The entry's own word sets it, as a field's name, a typed value or a condition's tests.
          this.pieces += 1;
          this.inside(at, given.value, inside, report, given.parameter === CONDITION_TESTS);
          break;
        case 'unset':
          this.gap({ kind: 'parameter', entry: call.entry, parameter: given.parameter }, at);
          break;
        case 'unoffered':
          this.fail(at, 'the engine does not offer a parameter that the document holds');
          break;
        case 'argument':
        case 'option':
          this.argument(at, parameter, given.value, inside, report);
          break;
        default:
          setting satisfies never;
      }
    }
  }
}

/** Rebuilds `documents` from what the model offers, with the catalogue `using` and the `inputs` the documents are run with. */
export function rebuildCorpus(
  documents: readonly CorpusDocument[],
  inputs: readonly InputView[],
  using: Catalogue,
): CorpusFound {
  const rebuild = new Rebuild(using, inputs);
  for (const held of documents) rebuild.document(held);
  return {
    gaps: rebuild.gaps,
    failures: rebuild.failures,
    pieces: rebuild.pieces,
    unplaced: rebuild.unplaced,
  };
}

/**
 * Returns the gaps found that `declared` lacks, and the gaps declared of a kind the corpus finds that were not
 * found, by cover key. A gap of a kind that another test finds is accepted for a piece the corpus
 * cannot build, and is not asked to be found.
 */
export function compareCorpus(
  found: CorpusFound,
  declared: readonly Gap[],
): { readonly undeclared: string[]; readonly stale: string[] } {
  const declaredKeys = new Set(declared.map((gap) => coverKey(gap.cover)));
  const ownKeys = declared
    .filter((gap) => FOUND_BY[gap.cover.kind] === 'corpus')
    .map((gap) => coverKey(gap.cover));
  return {
    undeclared: sorted([...found.gaps.keys()].filter((key) => !declaredKeys.has(key))),
    stale: sorted(ownKeys.filter((key) => !found.gaps.has(key))),
  };
}

/** The word the census uses for how a chain starts. */
function startWord(start: StartView, inputs: readonly InputView[]): string {
  switch (start.kind) {
    case 'hole':
      return 'hole';
    case 'input': {
      const input = inputs.find((held) => held.name === start.name);
      if (input?.kind === 'value') return 'connectedValue';
      return input?.kind === 'list' ? 'connectedList' : `input ${start.name}`;
    }
    case 'call': {
      if (fieldNameOf(start.call) !== null) return 'field';
      if (start.call.entry === 'polars.lit') {
        const kind = start.call.arguments[0]?.value.kind;
        if (kind === 'integer') return 'typed integer';
        if (kind === 'float') return 'typed float';
        if (kind === 'string') return 'typed text';
        if (kind === 'flag') return 'typed flag';
      }
      return `function ${start.call.entry}`;
    }
    default:
      return start satisfies never;
  }
}

/** What a corpus holds, in the words the checks of it use. */
export interface Held {
  /** Every entry any call of any document is of, a step, a start, a method or a link. */
  readonly entries: ReadonlySet<string>;
  /** Every step, by its entry. */
  readonly steps: ReadonlySet<string>;
  /** Every parameter given to a call, as the entry and the parameter. */
  readonly parameters: ReadonlySet<string>;
  /** How each chain starts, in the words of the census. */
  readonly starts: ReadonlySet<string>;
  /** The contexts the walk places a chain in, as `outer nested` or `outer alone`. */
  readonly contexts: ReadonlySet<string>;
  /** How many calls the documents hold, counting each step, start and link. */
  readonly calls: number;
  /** The outcomes of a step the reports give. */
  readonly outcomes: ReadonlySet<string>;
  /** The causes of a refusal the reports give. */
  readonly causes: ReadonlySet<string>;
}

/** Reads what `documents` hold, from the documents and their reports alone. */
export function heldBy(documents: readonly CorpusDocument[], inputs: readonly InputView[]): Held {
  const entries = new Set<string>();
  const steps = new Set<string>();
  const parameters = new Set<string>();
  const starts = new Set<string>();
  const contexts = new Set<string>();
  const outcomes = new Set<string>();
  const causes = new Set<string>();

  let calls = 0;
  function inCall(call: CallView): void {
    calls += 1;
    entries.add(call.entry);
    for (const given of call.arguments) {
      parameters.add(`${call.entry} ${given.parameter}`);
      inValue(given.value);
    }
  }
  function inValue(value: ArgumentView): void {
    switch (value.kind) {
      case 'expression': {
        starts.add(startWord(value.start, inputs));
        if (value.start.kind === 'call') inCall(value.start.call);
        value.links.forEach(inCall);
        return;
      }
      case 'many':
        value.items.forEach(inValue);
        return;
      case 'pairs':
        for (const entry of value.entries) {
          inValue(entry.key);
          inValue(entry.value);
        }
        return;
      default:
    }
  }

  for (const held of documents) {
    for (const step of held.document.steps) {
      steps.add(step.call.entry);
      inCall(step.call);
      // The calls on a group by that follow `group_by` hold what a step that groups is made of.
      step.links.forEach(inCall);
    }
    for (const report of held.run.steps) {
      outcomes.add(report.outcome.status);
      if (report.outcome.status === 'refused') causes.add(report.outcome.cause.kind);
      for (const walked of report.walk.expressions) {
        const { context } = walked;
        if (context !== null) contexts.add(`${context.outer} ${context.nested ?? 'alone'}`);
      }
    }
  }
  return { entries, steps, parameters, starts, contexts, calls, outcomes, causes };
}

/** What the engine and the host list that a corpus is to hold. */
export interface Wanted {
  /** The entries that stand in an expression, which the grid lists. */
  readonly expressionEntries: readonly string[];
  /** The catalogue's entries, with their parameters. */
  readonly catalogue: Catalogue;
  /** The contexts of the grid. */
  readonly contexts: ReadonlyArray<{ readonly outer: string; readonly nested: string | null }>;
  /** How a chain starts, in the census's words. */
  readonly starts: readonly string[];
  readonly outcomes: readonly string[];
  readonly causes: readonly string[];
}

/** Returns what `wanted` lists that `held` lacks, each as words that name it, sorted. */
export function missingFrom(held: Held, wanted: Wanted): string[] {
  const missing: string[] = [];
  for (const entry of wanted.expressionEntries) {
    if (!held.entries.has(entry)) missing.push(`entry ${entry}`);
  }
  for (const entry of wanted.catalogue.entries.values()) {
    if (entry.applies === 'frame' && !held.steps.has(entry.id)) missing.push(`step ${entry.id}`);
    for (const parameter of entry.parameters) {
      if (parameter.offered && !held.parameters.has(`${entry.id} ${parameter.name}`)) {
        missing.push(`parameter ${entry.id} ${parameter.name}`);
      }
    }
  }
  for (const context of wanted.contexts) {
    if (!held.contexts.has(`${context.outer} ${context.nested ?? 'alone'}`)) {
      missing.push(`context ${context.outer} ${context.nested ?? 'alone'}`);
    }
  }
  for (const start of wanted.starts) {
    if (!held.starts.has(start)) missing.push(`start ${start}`);
  }
  for (const outcome of wanted.outcomes) {
    if (!held.outcomes.has(outcome)) missing.push(`outcome ${outcome}`);
  }
  for (const cause of wanted.causes) {
    if (!held.causes.has(cause)) missing.push(`cause ${cause}`);
  }
  return sorted(missing);
}
