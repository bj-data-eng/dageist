/**
 * @file Test support: holds the census the host wrote, every variant the step editor has to
 * handle, against what the editor can draw. For each variant it asks the model's functions, which
 * the components call, whether the editor has a plan for it, and returns the variants it has none
 * for, and the gaps it finds in the census itself. A variant with no plan, and no declared gap, is
 * what the census test fails on. It is not part of the package's front door.
 */

import type {
  ArgumentView,
  InputView,
  KindView,
  ParameterView,
  ProblemView,
  RunView,
  ShapeView,
  TypedKind,
  ValueTypeView,
} from '@dageist/host';
import {
  DecodeError,
  decodeCatalogue,
  decodeEditAnswer,
  decodeNode,
  decodeRun,
} from '@dageist/host';

import catalogueFixture from '../../host/fixtures/catalogue.json';
import editEditedFixture from '../../host/fixtures/edit-edited.json';
import editRefusedFixture from '../../host/fixtures/edit-refused.json';
import { replaced } from '../../host/test-support/tamper';
import type { Census, CensusForm, CensusKind, CensusState } from '../../host/test-support/census';
import {
  controlOf,
  defaultWordOf,
  oneControlOf,
  pairsJoiner,
  startsWithEngineWord,
} from '../src/model/controls';
import { parameterSegment, pathKey } from '../src/model/doc';
import type { Handling } from '../src/model/edits';
import { handlingOf } from '../src/model/edits';
import { inputDetail, inputLines, originWords } from '../src/model/inputs';
import type { Cover, Gap } from '../src/model/limits';
import { FOUND_BY, coverKey } from '../src/model/limits';
import { isChanged, prepareRun, sayingOf } from '../src/model/report';
import { NULL_DASH, cellText, shownOf } from '../src/model/values';
import { valueDrawing } from '../src/model/words';
import { STARTS, grid } from './grid-gaps';
import { isDrawn } from './places';
import { sorted } from './sorted';

/** What a census holds that the editor has no plan for, and the gaps the census shows. */
export interface Planned {
  /** The variants the editor has no plan for, each as words, sorted. */
  readonly unplanned: readonly string[];
  /** The gaps found, by cover key. */
  readonly gaps: ReadonlyMap<string, Cover>;
}

/** Collects what is found of a census. */
class Findings {
  readonly unplanned: string[] = [];
  readonly gaps = new Map<string, Cover>();

  /** Notes a variant that has no plan. */
  lacks(variant: string): void {
    this.unplanned.push(variant);
  }

  /** Notes a gap, which a variant the editor cannot handle yet shows. */
  gap(cover: Cover): void {
    this.gaps.set(coverKey(cover), cover);
  }
}

/** The kind of start a kind of literal typed in makes, by the words of the census. */
const TYPED_STARTS: Readonly<Record<string, string>> = {
  integer: 'typedInteger',
  float: 'typedFloat',
  text: 'typedText',
  flag: 'typedFlag',
};

/** Whether the grid test runs a start of the kind `kind`, and of the entry `entry` where it is a function. */
function runsStart(kind: string, entry: string | null = null): boolean {
  return STARTS.some((start) => start.kind === kind && (entry === null || start.name === entry));
}

/** Returns the type of literal the census writes as `word`, or `null` where the contract has no such type. */
function valueTypeOf(word: string | null): ValueTypeView | null {
  switch (word) {
    case 'integer':
    case 'float':
    case 'string':
    case 'date':
    case 'any':
      return word;
    default:
      return null;
  }
}

/** Returns the kind of parameter the census writes as `kind`, or `null` where the contract has no such kind. */
function kindFrom(kind: CensusKind): KindView | null {
  switch (kind.kind) {
    case 'expression':
    case 'column':
    case 'field':
    case 'flag':
    case 'dtype':
    case 'duration':
    case 'table':
    case 'path':
      return { kind: kind.kind };
    case 'value': {
      const type = valueTypeOf(kind.type);
      return type === null ? null : { kind: 'value', type };
    }
    case 'choice':
      return { kind: 'choice', words: [] };
    case 'pattern':
      return { kind: 'pattern', plainTextFlag: null };
    case 'timeZone':
      return { kind: 'timeZone', none: null };
    case 'pairs': {
      const key = kind.key === null ? null : kindFrom(kind.key);
      const value = kind.value === null ? null : kindFrom(kind.value);
      return key === null || value === null ? null : { kind: 'pairs', key, value };
    }
    default:
      return null;
  }
}

/** Returns the shape the census writes as `shape`, or `null` where the contract has no such shape. */
function shapeFrom(shape: string): ShapeView | null {
  switch (shape) {
    case 'one':
    case 'many':
      return { shape };
    case 'perItem':
      return { shape: 'perItem', of: 'other' };
    default:
      return null;
  }
}

/** A parameter of `kind` held in `shape`. */
function parameterOf(shape: ShapeView, kind: KindView): ParameterView {
  return {
    name: 'parameter',
    shown: 'parameter',
    label: 'parameter',
    kind,
    shape,
    default: { kind: 'required' },
    defaultSetting: null,
    position: null,
    offered: true,
  };
}

/** The words that name a kind and a shape together, as the grid test names a kind of parameter that has no drawing. */
function formWords(form: CensusForm): string {
  return `${form.shape} of ${form.kind.kind}`;
}

/** Returns the slot of typed text as the kind of parameter it is typed into. */
function slotKind(slot: TypedKind): KindView {
  switch (slot.kind) {
    case 'expression':
      return { kind: 'expression' };
    case 'value':
      return { kind: 'value', type: slot.type };
    case 'duration':
      return { kind: 'duration' };
    case 'pattern':
      return { kind: 'pattern', plainTextFlag: null };
    case 'timeZone':
      return { kind: 'timeZone', none: null };
    case 'path':
      return { kind: 'path' };
    default:
      return slot satisfies never;
  }
}

/** Holds the starts, and the literals typed in, against the starts the grid test runs. */
function planStarts(census: Census, found: Findings): void {
  for (const start of census.starts) {
    switch (start.kind) {
      case 'field':
      case 'connectedValue':
      case 'connectedList':
        if (!runsStart(start.kind)) found.lacks(`start ${start.kind}`);
        break;
      case 'hole': {
        // Nothing picked yet is one word, drawn as a slot to pick from.
        const drawing = valueDrawing(null, true);
        if (drawing.drawn !== 'word' || drawing.shape.kind !== 'unset') {
          found.lacks('start hole');
        }
        break;
      }
      case 'typed': {
        const kind = Object.hasOwn(TYPED_STARTS, start.literal)
          ? TYPED_STARTS[start.literal]
          : null;
        if (kind === null || kind === undefined || !runsStart(kind)) {
          found.lacks(`start typed ${start.literal}`);
        }
        const value = census.typedValues.find((typed) => typed.literal === start.literal);
        if (value === undefined) found.lacks(`start typed ${start.literal}: no kind of value`);
        else if (!value.readable) found.gap({ kind: 'typed', literal: start.literal });
        break;
      }
      case 'function':
        if (!runsStart('function', start.entry)) found.lacks(`start function ${start.entry}`);
        break;
      default:
        found.lacks(`start ${JSON.stringify(start satisfies never)}`);
    }
  }
}

/** The argument a document holds for a number, text or flag typed in, as the census names its kind. */
function heldArgument(literal: string): ArgumentView | null {
  switch (literal) {
    case 'integer':
      return { kind: 'integer', shown: '1' };
    case 'float':
      return { kind: 'float', shown: '1.5' };
    case 'text':
      return { kind: 'string', value: 'a' };
    case 'flag':
      return { kind: 'flag', value: true };
    default:
      return null;
  }
}

/** Holds the numbers, texts and flags a document holds typed in, and the readings of typed text, against the starts and controls. */
function planTyped(census: Census, found: Findings): void {
  for (const value of census.typedValues) {
    const kind = Object.hasOwn(TYPED_STARTS, value.literal)
      ? TYPED_STARTS[value.literal]
      : undefined;
    const start = STARTS.find((candidate) => candidate.kind === kind);
    if (start === undefined || value.dtype === null) {
      found.lacks(`typed value ${value.literal}`);
    } else if (!start.leads({ kind: 'value', dtype: value.dtype, shape: 'one' })) {
      found.lacks(`typed value ${value.literal}: its start leads nowhere to ${value.dtype}`);
    }
    // A document that holds it must be shown by the words the model gives a typed value.
    const held = heldArgument(value.literal);
    if (held === null) found.lacks(`typed value ${value.literal}: no argument`);
    else if (shownOf(held) === null) found.gap({ kind: 'shown', what: `literal ${value.literal}` });
  }
  for (const reading of census.typedReadings) {
    const words = `reading ${JSON.stringify(reading.slot)} ${reading.literal ?? 'unread'}`;
    if (oneControlOf(slotKind(reading.slot)).control === 'none') {
      found.lacks(`${words}: no control`);
    }
    if (reading.literal === null) continue;
    const kind = Object.hasOwn(TYPED_STARTS, reading.literal)
      ? TYPED_STARTS[reading.literal]
      : undefined;
    if (kind === undefined) {
      found.lacks(`${words}: no start`);
    } else if (reading.slot.kind === 'expression' && reading.dtype !== null) {
      // A text typed where an expression starts is a start, which leads to the dtype the host gave it.
      const { dtype } = reading;
      const start = STARTS.find((candidate) => candidate.kind === kind);
      if (start === undefined || !start.leads({ kind: 'value', dtype, shape: 'one' })) {
        found.lacks(`${words}: its start leads nowhere to ${dtype}`);
      }
    }
  }
}

/** Holds the kinds, the shapes and the forms of parameter, and the kinds of option, against the controls. */
function planParameters(census: Census, found: Findings): void {
  const kindWords = new Set(census.parameterKinds);
  for (const word of census.parameterKinds) {
    const kind = kindFrom(
      word === 'pairs'
        ? {
            kind: 'pairs',
            type: null,
            key: { kind: 'column', type: null, key: null, value: null },
            value: { kind: 'flag', type: null, key: null, value: null },
          }
        : { kind: word, type: word === 'value' ? 'integer' : null, key: null, value: null },
    );
    if (kind === null) {
      found.lacks(`parameter kind ${word}`);
      continue;
    }
    if (!isDrawn(controlOf(parameterOf({ shape: 'one' }, kind)))) {
      found.lacks(`parameter kind ${word}: no control`);
    }
    // A flag and a choice start with the word the engine's table of options gives, and no other kind does.
    if (startsWithEngineWord(kind) !== census.optionKinds.includes(word)) {
      found.lacks(`option kind ${word}`);
    }
  }
  for (const word of census.optionKinds) {
    if (!kindWords.has(word)) found.lacks(`option kind ${word}: not a kind of parameter`);
  }
  for (const word of census.valueTypes) {
    const type = valueTypeOf(word);
    const control = type === null ? null : oneControlOf({ kind: 'value', type });
    if (control?.control !== 'literal' || control.type !== word) found.lacks(`value type ${word}`);
  }
  for (const word of census.parameterShapes) {
    if (shapeFrom(word) === null) found.lacks(`parameter shape ${word}`);
  }
  for (const form of census.parameterForms) {
    const shape = shapeFrom(form.shape);
    const kind = kindFrom(form.kind);
    if (shape === null || kind === null) {
      found.lacks(`parameter form ${formWords(form)}`);
    } else if (!isDrawn(controlOf(parameterOf(shape, kind)))) {
      found.gap({ kind: 'parameterKind', of: formWords(form) });
    }
  }
}

/** Holds the classes of dtype, the shapes and the kinds of place against the grid of places the engine was asked about. */
function planPlaces(census: Census, found: Findings): void {
  const classes = new Set<string>();
  const shapes = new Set<string>();
  const kinds = new Set<string>();
  const states = new Set<string>();
  for (const after of grid.afters) {
    kinds.add(after.kind);
    if (after.kind === 'value') {
      classes.add(after.dtype);
      shapes.add(after.shape);
    } else if (after.kind === 'condition') {
      states.add(after.state);
    }
  }
  const compared: ReadonlyArray<readonly [string, readonly string[], ReadonlySet<string>]> = [
    ['dtype class', census.dtypeClasses, classes],
    ['value shape', census.valueShapes, shapes],
    ['kind of place', census.afterKinds, kinds],
    ['state of a condition', census.conditionStates, states],
  ];
  for (const [what, words, ran] of compared) {
    for (const word of words) {
      if (!ran.has(word)) found.lacks(`${what} ${word}: not in the grid`);
    }
    for (const word of ran) {
      if (!words.includes(word)) found.lacks(`${what} ${word}: not in the census`);
    }
  }
}

/** The one step of a run the census builds states from, that Polars accepts and that counted its rows. */
type RawStep = Record<string, unknown>;

/** A raw step of a run, to be read by the host's decoder. */
function stepOf(base: RawStep, id: string, outcome: unknown, count: unknown): RawStep {
  return { ...base, id, outcome, count };
}

/** The cause of a refusal that the census word `cause` names, written as the host writes it. */
function causeOf(cause: string, refusal: unknown, reason: string): unknown {
  switch (cause) {
    case 'unknownTable':
    case 'unknownInput':
      return { kind: cause, name: 'nothing' };
    case 'build':
    case 'polars':
      return { kind: cause, reason };
    case 'notAllowed':
      return { kind: cause, refusal };
    default:
      return null;
  }
}

/** The place of a step the census builds, at the start of an expression. */
function placeOf(index: number, id: string): unknown {
  return { stepIndex: index, stepId: id, path: [{ kind: 'start' }] };
}

/** A refusal made of `source`, which says `reason`. */
function refusalOf(source: unknown, reason: string): unknown {
  return { source, reason };
}

/** What a valid, counted step is, as the host writes one: the base every other step is made from. */
function baseStep(): RawStep {
  return {
    id: 'base',
    summary: 'by a',
    lazy: true,
    outcome: { status: 'valid', columns: [{ name: 'a', dtype: 'Int64' }], changes: [] },
    count: { kind: 'counted', received: 2, kept: 1, first: [] },
    expressions: [],
    walk: { expressions: [], refusal: null, notKnown: null },
    atDefault: [],
  };
}

/** The counts of the rows, by the word of the census. */
function countOf(rows: string): unknown {
  switch (rows) {
    case 'notCounted':
      return { kind: 'notCounted' };
    case 'counted':
      return { kind: 'counted', received: 2, kept: 1, first: [] };
    case 'notReached':
      return { kind: 'notReached', failed: 'before' };
    case 'failed':
      return { kind: 'failed', reason: 'the rows could not be run' };
    default:
      return null;
  }
}

/** Builds the run that holds a step in `state`, preceded by the steps that make it so, or `null` where the state has no words in the contract. */
function runOf(state: CensusState): RunView | { readonly problem: string } {
  const base = baseStep();
  const valid = base.outcome;
  const before: RawStep[] = [];
  let blocker = 'before';
  switch (state.after) {
    case 'nothing':
      break;
    case 'refused':
      before.push(
        stepOf(
          base,
          'before',
          {
            status: 'refused',
            place: placeOf(0, 'before'),
            cause: causeOf('polars', null, 'the reason'),
          },
          { kind: 'notCounted' },
        ),
      );
      break;
    case 'incomplete':
      before.push(
        stepOf(
          base,
          'before',
          { status: 'incomplete', place: placeOf(0, 'before') },
          { kind: 'notCounted' },
        ),
      );
      break;
    case 'failed':
      before.push(stepOf(base, 'before', valid, countOf('failed')));
      break;
    case 'notRun':
      blocker = 'first';
      before.push(
        stepOf(
          base,
          'first',
          {
            status: 'refused',
            place: placeOf(0, 'first'),
            cause: causeOf('polars', null, 'the reason'),
          },
          { kind: 'notCounted' },
        ),
        stepOf(base, 'before', { status: 'notRun', blockedBy: 'first' }, { kind: 'notCounted' }),
      );
      break;
    default:
      return { problem: `what came before ${state.after} is not a state the census knows` };
  }
  const at = before.length;
  const refusal = refusalOf({ by: 'structure' }, 'the structure does not allow it');
  let outcome: unknown;
  switch (state.outcome) {
    case 'valid':
      outcome = valid;
      break;
    case 'refused':
      outcome = {
        status: 'refused',
        place: placeOf(at, 'tested'),
        cause: causeOf(state.cause ?? '', refusal, 'the reason'),
      };
      break;
    case 'incomplete':
      outcome = { status: 'incomplete', place: placeOf(at, 'tested') };
      break;
    case 'notRun':
      outcome = { status: 'notRun', blockedBy: blocker };
      break;
    default:
      return { problem: `the outcome ${state.outcome} is not one the census knows` };
  }
  const count = countOf(state.rows);
  if (count === null) {
    return { problem: `the rows state ${state.rows} is not one the census knows` };
  }
  const steps = [...before, stepOf(base, 'tested', outcome, count)];
  try {
    return decodeRun({ source: [{ name: 'a', dtype: 'Int64' }], steps });
  } catch (error) {
    if (error instanceof DecodeError) return { problem: error.message };
    throw error;
  }
}

/** What the editor says of the last step of `run`. */
function saidOf(run: RunView): ReturnType<typeof sayingOf> {
  const steps = run.steps.map((step) => ({
    id: step.id,
    call: { entry: 'LazyFrame.select', arguments: [] },
    links: [],
  }));
  const last = prepareRun(steps, run).steps.at(-1);
  if (last === undefined) throw new Error('the run has no step');
  return sayingOf(last);
}

/** Holds each state of a step, and each cause and source of a refusal, against the words the editor has for it. */
function planReports(census: Census, found: Findings): void {
  for (const rows of census.rowsStates) {
    if (countOf(rows) === null) found.lacks(`rows state ${rows}`);
  }
  for (const outcome of census.outcomes) {
    if (!['valid', 'refused', 'incomplete', 'notRun'].includes(outcome)) {
      found.lacks(`outcome ${outcome}`);
    }
  }
  for (const state of census.states) {
    const words =
      `state ${state.outcome} ${state.cause ?? ''} ${state.rows} after ${state.after}`.replace(
        '  ',
        ' ',
      );
    const run = runOf(state);
    if ('problem' in run) {
      found.lacks(`${words}: ${run.problem}`);
      continue;
    }
    const saying = saidOf(run);
    if (saying.says === 'problem' && saying.lines.some((line) => line.trim() === '')) {
      found.lacks(`${words}: a line with no words`);
    }
    const expectedWords =
      state.outcome !== 'valid' || state.rows === 'failed' || state.rows === 'notReached';
    if (expectedWords && saying.says !== 'problem') {
      found.lacks(`${words}: says nothing of a problem`);
    }
    if (state.rows === 'counted' && saying.says !== 'rows') found.lacks(`${words}: says no rows`);
    if (saying.says === 'nothing') {
      // A step accepted that was not counted, with no step before it that failed, has nothing to
      // say: the run read no rows.
      const readNoRows =
        state.outcome === 'valid' && state.rows === 'notCounted' && state.after === 'nothing';
      if (!readNoRows) found.lacks(`${words}: says nothing`);
    }
  }
  const refusals: ReadonlyArray<readonly [string, unknown]> = [
    ...census.causes.map((cause): readonly [string, unknown] => [
      `cause ${cause}`,
      causeOf(cause, refusalOf({ by: 'structure' }, 'the reason'), 'the reason'),
    ]),
    ...census.refusalSources.flatMap((source): ReadonlyArray<readonly [string, unknown]> => {
      switch (source) {
        case 'polars':
          return census.stages.map((stage) => [
            `refusal by polars at ${stage}`,
            causeOf('notAllowed', refusalOf({ by: 'polars', stage }, 'the reason'), ''),
          ]);
        case 'rule':
          return census.rules.map((rule) => [
            `refusal by rule ${rule.name}`,
            causeOf('notAllowed', refusalOf({ by: 'rule', rule: rule.name }, rule.statement), ''),
          ]);
        case 'structure':
          return [
            [
              'refusal by structure',
              causeOf('notAllowed', refusalOf({ by: 'structure' }, 'the reason'), ''),
            ],
          ];
        default:
          return [[`refusal source ${source}`, null]];
      }
    }),
  ];
  for (const [words, cause] of refusals) {
    if (cause === null) {
      found.lacks(words);
      continue;
    }
    const base = baseStep();
    const refused = stepOf(
      base,
      'tested',
      { status: 'refused', place: placeOf(0, 'tested'), cause },
      { kind: 'notCounted' },
    );
    let run: RunView;
    try {
      run = decodeRun({ source: [{ name: 'a', dtype: 'Int64' }], steps: [refused] });
    } catch (error) {
      if (error instanceof DecodeError) {
        found.lacks(`${words}: ${error.message}`);
        continue;
      }
      throw error;
    }
    const saying = saidOf(run);
    if (saying.says !== 'problem' || saying.lines.every((line) => line.trim() === '')) {
      found.lacks(`${words}: no words`);
    }
  }
  for (const rule of census.rules) {
    if (rule.statement.trim() === '') found.lacks(`rule ${rule.name}: says nothing`);
  }
  for (const unknown of census.notKnown) {
    if (unknown.because.trim() === '') found.lacks(`not known ${unknown.name}: says nothing`);
  }
}

/** The raw step the families below are made from. */
function plainStep(extra: RawStep): RawStep {
  return { ...baseStep(), id: 'tested', ...extra };
}

/** Reads a value the census builds with the host's decoder, or notes `words` as having no plan where it refuses it. */
function readOr<Read>(found: Findings, words: string, read: () => Read): Read | null {
  try {
    return read();
  } catch (error) {
    if (error instanceof DecodeError) {
      found.lacks(`${words}: ${error.message}`);
      return null;
    }
    throw error;
  }
}

/** The resolution of an expression the census names, as the host writes one. */
function resolutionOf(word: string): unknown {
  switch (word) {
    case 'dtype':
      return { kind: word, dtype: 'Int64' };
    case 'several':
      return { kind: word, dtypes: ['Int64', 'String'] };
    case 'unfinished':
    case 'noContext':
      return { kind: word };
    case 'refused':
      return { kind: word, reason: 'the reason' };
    default:
      return null;
  }
}

/** The default of a parameter the census names, as the catalogue writes one. */
function defaultOf(word: string): unknown {
  switch (word) {
    case 'required':
    case 'none':
    case 'empty':
    case 'other':
      return { kind: word };
    case 'flag':
      return { kind: word, value: true };
    case 'integer':
      return { kind: word, value: 1 };
    case 'float':
      return { kind: word, value: 1.5 };
    case 'text':
      return { kind: word, value: 'a' };
    default:
      return null;
  }
}

/** The setting of a parameter the census names, as the catalogue writes one. */
function settingOf(word: string): unknown {
  const unset = { kind: 'unset' };
  switch (word) {
    case 'unset':
      return unset;
    case 'flag':
      return { kind: word, value: true };
    case 'integer':
      return { kind: word, value: 1, shown: '1' };
    case 'float':
      return { kind: word, value: 1.5, shown: '1.5' };
    case 'text':
      return { kind: word, value: 'a', shown: 'a' };
    case 'word':
      return { kind: word, value: 'a' };
    case 'field':
      return { kind: word, column: 'a' };
    case 'element':
      return { kind: word, method: 'first' };
    case 'comparison':
      return { kind: word, entry: 'Expr.eq', field: unset, other: unset };
    case 'list':
      return { kind: word, items: [] };
    case 'pairs':
      return { kind: word, entries: [] };
    default:
      return null;
  }
}

/** The reason for a refusal of an edit the census names, as the host writes one. */
function problemOf(kind: string, found: unknown, needs: string): unknown {
  switch (kind) {
    case 'noStep':
      return { kind, steps: 1 };
    case 'stepChanged':
      return { kind, found: 'a' };
    case 'nothing':
    case 'notPerItem':
    case 'noDefault':
      return { kind };
    case 'doesNotFit':
      return { kind, found };
    case 'wrongPlace':
      return { kind, needs, found };
    case 'outOfRange':
      return { kind, index: 3, len: 2 };
    case 'unknownEntry':
    case 'noStart':
      return { kind, entry: 'Expr.a' };
    case 'cannotAdd':
    case 'refused':
      return { kind, reason: 'the reason' };
    case 'needsValue':
      return { kind, parameter: 'a' };
    default:
      return null;
  }
}

/** The catalogue with the field `field` of its first parameter changed to `value`. */
function withParameter(field: string, value: unknown): unknown {
  return replaced(catalogueFixture, ['entries', 0, 'parameters', 0, field], value);
}

/** What the editor shows as the dtype after the start of the expression at `key` in the only step of `run`, or `null`. */
export type DtypeShown = (run: RunView, key: string) => string | null;

/** What the editor shows, as the model prepares the run. */
const PREPARED_DTYPE: DtypeShown = (run, key) =>
  prepareRun([], run).steps[0]?.chains.get(key)?.source ?? null;

/** Holds how a column changed, how an expression resolved, what a parameter holds by default and how an edit came out against the model's functions. */
function planFamilies(census: Census, found: Findings, dtypeShown: DtypeShown): void {
  const source = [{ name: 'a', dtype: 'Int64' }];
  const place = { stepIndex: 0, stepId: 'tested', path: [parameterSegment('exprs')] };
  for (const change of census.changes) {
    const words = `change ${change}`;
    const outcome = {
      status: 'valid',
      columns: source,
      changes: [{ name: 'a', change }],
    };
    const run = readOr(found, words, () => decodeRun({ source, steps: [plainStep({ outcome })] }));
    const step = run === null ? undefined : prepareRun([], run).steps[0];
    if (step !== undefined && !isChanged(step.changes, 'a')) found.lacks(`${words}: not marked`);
  }
  for (const word of census.resolutions) {
    const words = `resolution ${word}`;
    const resolution = resolutionOf(word);
    if (resolution === null) {
      found.lacks(words);
      continue;
    }
    const expressions = [{ place, start: resolution, links: [resolution] }];
    const run = readOr(found, words, () =>
      decodeRun({ source, steps: [plainStep({ expressions })] }),
    );
    if (run === null) continue;
    const dtype = dtypeShown(run, pathKey(place.path));
    if (word === 'dtype' && dtype !== 'Int64') found.lacks(`${words}: no dtype shown`);
    // The dtypes of a chain that gives several columns are not shown, which is a gap; what is
    // unfinished, has no context or is refused has none to show, and the step says why.
    if (word === 'several' && dtype === null) {
      found.gap({ kind: 'shown', what: 'resolution several' });
    }
  }
  for (const word of census.defaultKinds) {
    const words = `default ${word}`;
    const held = defaultOf(word);
    if (held === null) {
      found.lacks(words);
      continue;
    }
    const read = readOr(found, words, () => decodeCatalogue(withParameter('default', held)));
    const parameter = read?.entries[0]?.parameters[0];
    if (parameter !== undefined && pairsJoiner(parameter) !== (word === 'empty' ? '=' : '→')) {
      found.lacks(`${words}: joins pairs by the wrong word`);
    }
  }
  for (const word of census.settingKinds) {
    const words = `setting ${word}`;
    const held = settingOf(word);
    if (held === null) {
      found.lacks(words);
      continue;
    }
    const read = readOr(found, words, () => decodeCatalogue(withParameter('defaultSetting', held)));
    const parameter = read?.entries[0]?.parameters[0];
    // A flag and a word are held as one the controls draw; any other setting is no default word.
    if (
      parameter !== undefined &&
      (defaultWordOf(parameter) !== '') !== ['flag', 'word'].includes(word)
    ) {
      found.lacks(`${words}: is held as a word, or is not`);
    }
  }
  for (const word of census.editAnswers) {
    const fixture =
      word === 'edited' ? editEditedFixture : word === 'refused' ? editRefusedFixture : null;
    if (fixture === null) {
      found.lacks(`edit answer ${word}`);
      continue;
    }
    const answer = readOr(found, `edit answer ${word}`, () => decodeEditAnswer(fixture));
    if (answer !== null && answer.status !== word) {
      found.lacks(`edit answer ${word}: reads as ${answer.status}`);
    }
  }
  /**
   * Reads an edit refused for `problem` with the host's decoder, which must read it as the thing
   * `is` says, and holds what the editor does with it to `expected`, which is written here by hand:
   * it asks for the value of an option that starts with none, and tells the person every other
   * refusal.
   */
  const refusedFor = (
    words: string,
    problem: unknown,
    expected: Handling,
    is: (read: ProblemView) => boolean,
  ): void => {
    if (problem === null) {
      found.lacks(words);
      return;
    }
    const answer = readOr(found, words, () =>
      decodeEditAnswer(replaced(editRefusedFixture, ['problem'], problem)),
    );
    if (answer?.status !== 'refused') return;
    if (!is(answer.problem)) found.lacks(`${words}: is read as ${answer.problem.kind}`);
    if (handlingOf(answer.problem) !== expected) {
      found.lacks(`${words}: is not handled as ${expected}`);
    }
  };
  const step = { kind: 'step' };
  for (const kind of census.problemKinds) {
    refusedFor(
      `problem ${kind}`,
      problemOf(kind, step, 'call'),
      kind === 'needsValue' ? 'askForValue' : 'tell',
      (read) => read.kind === kind,
    );
  }
  const someValue = census.valueKinds[0] ?? 'expression';
  for (const thing of census.thingKinds) {
    const held = thing === 'value' ? { kind: thing, of: someValue } : { kind: thing };
    refusedFor(
      `thing ${thing}`,
      problemOf('doesNotFit', held, 'call'),
      'tell',
      (read) => read.kind === 'doesNotFit' && read.found.kind === thing,
    );
  }
  for (const kind of census.valueKinds) {
    refusedFor(
      `value kind ${kind}`,
      problemOf('doesNotFit', { kind: 'value', of: kind }, 'call'),
      'tell',
      (read) => read.kind === 'doesNotFit' && read.found.kind === 'value' && read.found.of === kind,
    );
  }
  for (const needs of census.wanted) {
    refusedFor(
      `wanted ${needs}`,
      problemOf('wrongPlace', step, needs),
      'tell',
      (read) => read.kind === 'wrongPlace' && read.needs === needs,
    );
  }
}

/** The origin the census writes as `origin`, as the host writes it. */
function originFrom(origin: string): unknown {
  switch (origin) {
    case 'node':
      return { kind: 'node', name: 'a node' };
    case 'column':
      return { kind: 'column', node: 'a node', column: 'a column' };
    case 'parameter':
      return { kind: 'parameter' };
    default:
      return null;
  }
}

/** The input the census writes as `kind`, from `origin`, as the host writes it. */
function inputFrom(kind: string, from: unknown): unknown {
  switch (kind) {
    case 'table':
      return { kind, name: 'input', from, columns: [{ name: 'a', dtype: 'Int64' }] };
    case 'value':
      return { kind, name: 'input', from, dtype: 'Int64', value: null };
    case 'list':
      return { kind, name: 'input', from, dtype: 'Int64', values: { shown: ['1', null], more: 1 } };
    default:
      return null;
  }
}

/** Reads one input of a node, from `origin`, as the host's decoder reads it. */
function readInput(kind: string, origin: string): InputView | string {
  const from = originFrom(origin);
  const input = inputFrom(kind, from);
  if (from === null || input === null) return `${kind} from ${origin}`;
  try {
    const node = decodeNode({
      source: { name: 'source', from, columns: [] },
      inputs: [input],
    });
    const [only] = node.inputs;
    return only === undefined ? `${kind} from ${origin}: the node holds none` : only;
  } catch (error) {
    if (error instanceof DecodeError) return `${kind} from ${origin}: ${error.message}`;
    throw error;
  }
}

/** Holds each kind of input, and where it comes from, and each kind of cell, against the words the editor shows them with. */
function planRows(census: Census, found: Findings): void {
  for (const kind of census.inputKinds) {
    for (const origin of census.origins) {
      const input = readInput(kind, origin);
      if (typeof input === 'string') {
        found.lacks(`input ${input}`);
        continue;
      }
      const lines = inputLines(input);
      if (
        lines.length < 2 ||
        lines.some((line) => line.trim() === '') ||
        inputDetail(input) === ''
      ) {
        found.lacks(`input ${kind} from ${origin}: no words`);
      }
      if (originWords(input.from).trim() === '') found.lacks(`origin ${origin}: no words`);
    }
  }
  for (const cell of census.cells) {
    switch (cell) {
      case 'text':
        if (cellText('a value') !== 'a value') found.lacks('cell text');
        break;
      case 'null':
        if (cellText(null) !== NULL_DASH) found.lacks('cell null');
        break;
      default:
        found.lacks(`cell ${cell}`);
    }
  }
}

/**
 * Holds every variant of `census` against what the editor can draw, and returns those it has no
 * plan for and the gaps the census shows. `dtypeShown` says what the editor shows as a dtype, which
 * a test may replace to see a gap go stale.
 */
export function planFor(census: Census, dtypeShown: DtypeShown = PREPARED_DTYPE): Planned {
  const found = new Findings();
  planStarts(census, found);
  planTyped(census, found);
  planParameters(census, found);
  planPlaces(census, found);
  planReports(census, found);
  planFamilies(census, found, dtypeShown);
  planRows(census, found);
  return { unplanned: sorted(found.unplanned), gaps: found.gaps };
}

/**
 * Returns the gaps `planned` found that `declared` lacks, and the gaps declared of the kinds the
 * census finds that it did not find. A gap of a kind another test finds is left to that test.
 */
export function compareCensus(
  planned: Planned,
  declared: readonly Gap[],
): { readonly undeclared: string[]; readonly stale: string[] } {
  const declaredKeys = new Set(declared.map((gap) => coverKey(gap.cover)));
  const mine = new Set(
    declared
      .filter((gap) => FOUND_BY[gap.cover.kind] === 'census')
      .map((gap) => coverKey(gap.cover)),
  );
  return {
    undeclared: sorted([...planned.gaps.keys()].filter((key) => !declaredKeys.has(key))),
    stale: sorted([...mine].filter((key) => !planned.gaps.has(key))),
  };
}
