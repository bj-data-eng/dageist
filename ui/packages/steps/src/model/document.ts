/**
 * @file Writes the editor's steps as the step document the engine runs: the
 * JSON the host's `run_document` and `rows_after_step` commands take. The
 * translation is pure: it needs the catalogue and the steps, and gives text
 * and what is needed to read the engine's report back onto the editor.
 *
 * A step the document cannot hold yet, because something in it is still to be
 * picked or the engine's catalogue has no entry for it, is held back with the
 * reason, and every step after it waits: a document holds the steps up to the
 * first one that is held, as the engine itself stops at the first it refuses.
 *
 * Where the engine's report names a place in the document, the marks written
 * here say which word of the editor it is, so that the editor can show a
 * refusal on the word it marks.
 */

import type { EntryView, KindView, ParameterView, SegmentView } from '@dageist/host';

import type { Catalogue } from './catalogue';
import { verbEntry } from './catalogue';
import type { Method } from './expr';
import { argumentRule, methodOptionParameters } from './expr';
import { assertNever } from './frame';
import type { Options, Setting } from './settings';
import { isComplete, optionOf, setParameters } from './settings';
import type { Cell, Chain, Comparison, Condition, Source, Value } from './source';
import { IS_IN } from './source';
import type { Step } from './steps';
import { keysOf, stepItems } from './steps';

/** The version of the document format the engine reads. */
const DOCUMENT_VERSION = 1;

/** A piece of JSON, as the document is built before it is written as text. */
type Json = null | boolean | number | string | readonly Json[] | { readonly [key: string]: Json };

/** Which word of the editor a place in the document is. */
export type Mark =
  /** The word a chain starts from. */
  | { readonly kind: 'start'; readonly chainId: string }
  /** A method of a chain. */
  | { readonly kind: 'method'; readonly methodId: string }
  /** The comparison of a test: its field, its sign and its value. */
  | { readonly kind: 'test'; readonly testId: string };

/** The name a word of the editor is marked wrong by, for the parts that draw it. */
export function wrongId(mark: Mark): string {
  switch (mark.kind) {
    case 'start':
      return `start:${mark.chainId}`;
    case 'method':
      return `method:${mark.methodId}`;
    case 'test':
      return `test:${mark.testId}`;
    default:
      return assertNever(mark);
  }
}

/** The key a place in a document is kept under, made of the words of its segments. */
export function placeKey(segments: readonly string[]): string {
  return segments.join('/');
}

/** Returns one step of a place as the word its key is made of. */
export function segmentWord(segment: SegmentView): string {
  switch (segment.kind) {
    case 'parameter':
      return `parameter:${segment.name}`;
    case 'item':
      return `item:${String(segment.index)}`;
    case 'key':
      return `key:${String(segment.index)}`;
    case 'value':
      return `value:${String(segment.index)}`;
    case 'start':
      return 'start';
    case 'link':
      return `link:${String(segment.index)}`;
    default:
      return assertNever(segment);
  }
}

/**
 * How a chain of the editor sits in the document: the expression is at a
 * place of its own, its links are what follows its start. A chain that starts
 * from `when` has its `then` and `otherwise` first among them.
 */
export interface ChainOwner {
  readonly chainId: string;
  /** How many links come before the first method: 2 after `when`, otherwise 0. */
  readonly whenLinks: number;
  /** The method of each link after those, in order. */
  readonly methodIds: readonly string[];
}

/** What the engine's report of a step is read with. */
export interface SentStep {
  readonly kind: 'sent';
  /** The step as the document holds it, as JSON text, to tell whether an edit changed it. */
  readonly canonical: string;
  /** Which word of the editor a place of the step is, by the key of its path. */
  readonly marks: ReadonlyMap<string, Mark>;
  /** Which chain of the editor an expression of the step is, by the key of its path. */
  readonly chains: ReadonlyMap<string, ChainOwner>;
  /**
   * What is still to be picked, by the key of the place of each hole the step holds: the start
   * of the expression that stands for it. The engine reports the first of them when it says the
   * step is incomplete.
   */
  readonly holes: ReadonlyMap<string, HeldProblem>;
}

/** Why a step is not in the document. */
export interface HeldProblem {
  /** What is the matter: something still to be picked, or something the engine cannot be given yet. */
  readonly cause: 'unset' | 'unsupported';
  /** What to tell the person. */
  readonly message: string;
  /** The words of the editor that are the matter, as `wrongId` names them. */
  readonly wrong: readonly string[];
}

/** A step that is left out of the document, with why. */
export interface HeldStep {
  readonly kind: 'held';
  readonly problem: HeldProblem;
}

/** A step that is left out because a step before it is. */
export interface WaitingStep {
  readonly kind: 'waiting';
  /** The identifier of the step that is held. */
  readonly blockedBy: string;
}

/** What became of one step of the editor. */
export type StepTranslation = SentStep | HeldStep | WaitingStep;

/** The editor's steps as a document, and what became of each. */
export interface Translation {
  /** The document's JSON text: the steps up to the first one that is held. */
  readonly text: string;
  /** One for each step of the editor, in its order. */
  readonly steps: readonly StepTranslation[];
}

/** Raised inside the translation when a step cannot be written, and caught at that step. */
class Held extends Error {
  constructor(readonly problem: HeldProblem) {
    super(problem.message);
    this.name = 'Held';
  }
}

function unset(message: string, wrong: readonly string[] = []): Held {
  return new Held({ cause: 'unset', message, wrong });
}

function unsupported(message: string, wrong: readonly string[] = []): Held {
  return new Held({ cause: 'unsupported', message, wrong });
}

/** What the translation of one step builds up. */
interface Notes {
  readonly marks: Map<string, Mark>;
  readonly chains: Map<string, ChainOwner>;
  readonly holes: Map<string, HeldProblem>;
}

/** Everything a step is written with. */
interface Context {
  readonly catalogue: Catalogue;
  readonly notes: Notes;
}

type Path = readonly string[];

function under(path: Path, ...more: readonly string[]): Path {
  return [...path, ...more];
}

function markPlace(context: Context, path: Path, found: Mark): void {
  context.notes.marks.set(placeKey(path), found);
}

/** The step of the way `Json` for a call: its entry, and the arguments it is given. */
function call(entry: string, args: Readonly<Record<string, Json>>): Json {
  return { entry, arguments: args };
}

function columnsOf(...names: readonly string[]): Json {
  return { columns: names };
}

/** Returns a cell as the argument of `polars.lit`. */
function literalArgument(
  value: Exclude<Cell, null>,
  float: boolean,
  wrong: readonly string[],
): Json {
  if (typeof value === 'string') return { string: value };
  if (!Number.isFinite(value)) throw unsupported('A number must be finite.', wrong);
  return float || !Number.isSafeInteger(value) ? { float: value } : { integer: value };
}

/**
 * Returns the identifier of the entry a comparison is written as, or `null` for `IS_IN`, which the
 * catalogue has no entry for yet. A comparison is held as the entry that compares.
 */
export function comparisonEntryId(comparison: Comparison): string | null {
  return comparison === IS_IN ? null : comparison;
}

/** An expression before it is written: where it starts, and the links that follow. */
interface Body {
  readonly start: Json;
  readonly links: readonly Json[];
}

/** Writes an expression as the document holds it: where it starts, then the links that follow. */
function expressionJson(start: Json, links: readonly Json[]): Json {
  // The document names the links of an expression `then`.
  // oxlint-disable-next-line unicorn/no-thenable
  return { start, then: links };
}

/** What an expression that is still to be picked starts from: nothing, which the engine reports as incomplete. */
const HOLE: Json = { hole: null };

/**
 * Notes that something is still to be picked at the start of the expression at `path`, and says
 * what, by the word of the editor that is the matter.
 */
function holeAt(context: Context, path: Path, message: string, wrong: readonly string[]): Json {
  context.notes.holes.set(placeKey(under(path, 'start')), { cause: 'unset', message, wrong });
  return HOLE;
}

/** Writes a value as an expression's body: where it starts, then the links that follow. */
function expressionBody(
  context: Context,
  value: Value,
  path: Path,
  wrong: readonly string[],
): Body {
  switch (value.kind) {
    case 'unset':
      return { start: holeAt(context, path, 'Pick a value.', wrong), links: [] };
    case 'column':
      if (value.column === '') {
        return { start: holeAt(context, path, 'Pick a field.', wrong), links: [] };
      }
      return { start: call('polars.col', { name: columnsOf(value.column) }), links: [] };
    case 'literal':
      if (value.value === null) {
        throw unsupported('The engine does not take None as a value yet.', wrong);
      }
      return {
        start: call('polars.lit', {
          value: literalArgument(value.value, value.float === true, wrong),
        }),
        links: [],
      };
    case 'input':
      if (value.name === '') {
        return { start: holeAt(context, path, 'Pick an input.', wrong), links: [] };
      }
      return { start: { input: value.name }, links: [] };
    case 'chain':
      return chainBody(context, value.chain, value.id, path);
    default:
      return assertNever(value);
  }
}

/** Writes a value as the argument of a parameter that takes an expression. */
function expressionArgument(
  context: Context,
  value: Value,
  path: Path,
  wrong: readonly string[] = [],
): Json {
  const { start, links } = expressionBody(context, value, path, wrong);
  return { expression: expressionJson(start, links) };
}

/** Writes the argument of a method for the parameter its entry takes first. */
function methodArgument(
  context: Context,
  parameter: ParameterView,
  method: Method,
  path: Path,
  wrong: readonly string[],
): Json {
  const given = method.argument ?? { kind: 'unset' };
  if (given.kind === 'unset') {
    // Still to be picked: a hole stands for it, so the method before it is still walked.
    const message = `Give ${method.name} a value or a field (${parameter.label}).`;
    const hole = (at: Path): Json => ({
      expression: expressionJson(holeAt(context, at, message, wrong), []),
    });
    return parameter.shape.shape === 'many' ? { many: [hole(under(path, 'item:0'))] } : hole(path);
  }
  if (parameter.shape.shape === 'many') {
    return { many: [expressionArgument(context, given, under(path, 'item:0'), wrong)] };
  }
  return expressionArgument(context, given, path, wrong);
}

/** Writes one method of a chain as a call, with its argument and the parameters it is set with. */
function methodCall(context: Context, method: Method, link: number, path: Path): Json {
  const here = under(path, `link:${String(link)}`);
  const wrong = [wrongId({ kind: 'method', methodId: method.id })];
  markPlace(context, here, { kind: 'method', methodId: method.id });
  const entry = verbEntryOfMethod(context, method);
  const rule = argumentRule(context.catalogue, method.name);
  const args: Record<string, Json> = {};
  if (rule !== null) {
    const parameter = rule.parameter.name;
    args[parameter] = methodArgument(
      context,
      rule.parameter,
      method,
      under(here, `parameter:${parameter}`),
      wrong,
    );
  }
  Object.assign(
    args,
    optionArguments(
      context,
      entry,
      method.options,
      methodOptionParameters(context.catalogue, method.name),
      () => [],
      here,
      wrong,
    ),
  );
  return call(entry.id, args);
}

function verbEntryOfMethod(context: Context, method: Method): EntryView {
  const entry = context.catalogue.entries.get(`Expr.${method.name}`);
  if (entry === undefined) {
    throw unsupported(`The catalogue has no entry Expr.${method.name}, so it cannot be run.`, [
      wrongId({ kind: 'method', methodId: method.id }),
    ]);
  }
  return entry;
}

/**
 * Writes the predicates of a condition, which `when` and `filter` hold and
 * each of which must be true, onto `written`. The list is at `listPath`, so
 * each predicate is the item at its place there.
 */
function collectPredicates(
  context: Context,
  condition: Condition,
  listPath: Path,
  written: Json[],
): void {
  if (condition.kind === 'group') {
    if (condition.items.length === 0) throw unset('Pick a condition.');
    // Polars takes several predicates as all of them, so only a group that needs all of its items,
    // or has one, can be written; one that needs any is `|`, which the catalogue has no entry for.
    if (!condition.every && condition.items.length > 1) {
      throw unsupported('The engine has no "any of" for conditions yet.');
    }
    for (const item of condition.items) collectPredicates(context, item, listPath, written);
    return;
  }
  const path = under(listPath, `item:${String(written.length)}`);
  written.push(
    condition.kind === 'chain'
      ? expressionArgument(context, chainValue(condition), path)
      : testPredicate(context, condition, path),
  );
}

/** Writes a condition as the list of predicates `when` and `filter` take. */
function predicates(context: Context, condition: Condition, listPath: Path): Json {
  const written: Json[] = [];
  collectPredicates(context, condition, listPath, written);
  return { many: written };
}

function chainValue(condition: { readonly id: string; readonly chain: Chain }): Value {
  return { kind: 'chain', id: condition.id, chain: condition.chain };
}

/** Writes a comparison as an expression: the field, then the call that compares it. */
function testPredicate(
  context: Context,
  test: Extract<Condition, { kind: 'test' }>,
  path: Path,
): Json {
  const wrong = [wrongId({ kind: 'test', testId: test.id })];
  const comparison = comparisonEntryId(test.comparison);
  if (comparison === null) {
    throw unsupported(`The engine has no ${IS_IN} for conditions yet.`, wrong);
  }
  markPlace(context, under(path, 'start'), { kind: 'test', testId: test.id });
  markPlace(context, under(path, 'link:0'), { kind: 'test', testId: test.id });
  const other = expressionArgument(
    context,
    test.value,
    under(path, 'link:0', 'parameter:other'),
    wrong,
  );
  const start =
    test.column === ''
      ? holeAt(context, path, 'Pick the field to compare.', wrong)
      : call('polars.col', { name: columnsOf(test.column) });
  return {
    expression: expressionJson(start, [call(comparison, { other })]),
  };
}

/** Writes the start of a chain, and for `when` the links that complete it. */
function chainStart(
  context: Context,
  source: Source,
  chainId: string,
  path: Path,
): { readonly start: Json; readonly links: readonly Json[] } {
  const wrong = [wrongId({ kind: 'start', chainId })];
  switch (source.kind) {
    case 'column':
      if (source.column === '') {
        return {
          start: holeAt(context, path, 'Pick the field the chain starts from.', wrong),
          links: [],
        };
      }
      return {
        start: call('polars.col', { name: columnsOf(source.column) }),
        links: [],
      };
    case 'element':
      return { start: call('polars.element', {}), links: [] };
    case 'input':
      if (source.name === '') {
        return {
          start: holeAt(context, path, 'Pick the input the chain starts from.', wrong),
          links: [],
        };
      }
      return { start: { input: source.name }, links: [] };
    case 'when': {
      const [arm, ...further] = source.arms;
      if (arm === undefined) throw unset('Pick a condition.', wrong);
      // A further case is a `when` after `otherwise`; the document has no place for it here.
      if (further.length > 0) {
        throw unsupported(
          'The engine takes one case for a when; a further case follows otherwise.',
          wrong,
        );
      }
      const start = call('polars.when', {
        predicates: predicates(
          context,
          arm.condition,
          under(path, 'start', 'parameter:predicates'),
        ),
      });
      const statement = (value: Value, link: number): Json =>
        expressionArgument(
          context,
          value,
          under(path, `link:${String(link)}`, 'parameter:statement'),
          wrong,
        );
      return {
        start,
        links: [
          call('When.then', { statement: statement(arm.returns, 0) }),
          call('Then.otherwise', { statement: statement(source.otherwise, 1) }),
        ],
      };
    }
    default:
      return assertNever(source);
  }
}

/** Writes a chain as an expression's body and notes where it sits. */
function chainBody(context: Context, chain: Chain, chainId: string, path: Path): Body {
  markPlace(context, under(path, 'start'), { kind: 'start', chainId });
  const { start, links } = chainStart(context, chain.source, chainId, path);
  context.notes.chains.set(placeKey(path), {
    chainId,
    whenLinks: links.length,
    methodIds: chain.methods.map((method) => method.id),
  });
  const methods = chain.methods.map((method, index) =>
    methodCall(context, method, links.length + index, path),
  );
  return { start, links: [...links, ...methods] };
}

/** Writes a setting as the argument of a parameter of `kind`. */
function settingArgument(
  context: Context,
  kind: KindView,
  setting: Setting,
  path: Path,
  items: readonly string[],
  parameter: ParameterView,
  wrong: readonly string[],
): Json {
  switch (setting.kind) {
    case 'unset':
      throw unset(`Give ${parameter.shown} a value (${parameter.label}).`, wrong);
    case 'flag':
      return { flag: setting.value };
    case 'number':
      return numberArgument(kind, setting.value, setting.float === true, wrong);
    case 'text':
      if (kind.kind === 'duration') return { duration: setting.value };
      if (kind.kind === 'path') return { path: setting.value };
      return { string: setting.value };
    case 'word':
      switch (kind.kind) {
        case 'choice':
          return { choice: setting.value };
        case 'dtype':
          return { dtype: setting.value };
        case 'column':
          return columnsOf(setting.value);
        case 'table':
          return { table: setting.value };
        default:
          return { string: setting.value };
      }
    case 'expression':
      return expressionArgument(context, setting.value, path, wrong);
    case 'list':
      return {
        many: setting.items.map((item, index) =>
          settingArgument(
            context,
            kind,
            item,
            under(path, `item:${String(index)}`),
            items,
            parameter,
            wrong,
          ),
        ),
      };
    case 'pairs': {
      const key = kind.kind === 'pairs' ? kind.key : kind;
      const value = kind.kind === 'pairs' ? kind.value : kind;
      return {
        pairs: setting.entries.map((entry, index) => [
          settingArgument(
            context,
            key,
            entry.key,
            under(path, `key:${String(index)}`),
            [],
            parameter,
            wrong,
          ),
          settingArgument(
            context,
            value,
            entry.value,
            under(path, `value:${String(index)}`),
            [],
            parameter,
            wrong,
          ),
        ]),
      };
    }
    case 'each':
      // Every item has its place in the list; one with no setting of its own is at the default.
      return {
        many: items.map((item, index) => {
          const own = Object.hasOwn(setting.items, item) ? setting.items[item] : undefined;
          // An item with no setting of its own holds the default the engine reports.
          const held = parameter.defaultSetting;
          const fallback = held?.kind === 'flag' ? { flag: held.value } : null;
          if (own === undefined) {
            if (fallback === null) {
              throw unset(`Give ${parameter.shown} a value for ${item}.`, wrong);
            }
            return fallback;
          }
          return settingArgument(
            context,
            kind,
            own,
            under(path, `item:${String(index)}`),
            [],
            parameter,
            wrong,
          );
        }),
      };
    default:
      return assertNever(setting);
  }
}

/** Writes a number as the argument of a parameter of `kind`: a whole number or one with a fraction. */
function numberArgument(
  kind: KindView,
  value: number,
  float: boolean,
  wrong: readonly string[],
): Json {
  if (!Number.isFinite(value)) throw unsupported('A number must be finite.', wrong);
  if (kind.kind === 'value' && kind.type === 'float') return { float: value };
  if (kind.kind === 'value' && kind.type === 'integer') return { integer: value };
  return float || !Number.isSafeInteger(value) ? { float: value } : { integer: value };
}

/**
 * Writes the arguments of the parameters that `options` sets. Whether one is
 * what Polars does when it is not given is the engine's to say, and it says so
 * in its report.
 */
function optionArguments(
  context: Context,
  entry: EntryView,
  options: Options,
  allowed: readonly ParameterView[],
  items: (of: string) => readonly string[],
  base: Path,
  wrong: readonly string[],
): Record<string, Json> {
  const written: Record<string, Json> = {};
  for (const { parameter, setting } of setParameters(entry, options)) {
    if (!allowed.some((offered) => offered.name === parameter.name)) continue;
    const each = parameter.shape.shape === 'perItem' ? items(parameter.shape.of) : [];
    written[parameter.name] = settingArgument(
      context,
      parameter.kind,
      setting,
      under(base, `parameter:${parameter.name}`),
      each,
      parameter,
      wrong,
    );
  }
  return written;
}

/** The parameters each step draws by its own structure, which are not options. */
const COVERED: Readonly<Record<Step['verb'], readonly string[]>> = {
  with_columns: ['exprs', 'named_exprs'],
  filter: ['predicates'],
  sort: ['by', 'more_by'],
  select: ['exprs'],
  rename: ['mapping'],
  join: ['other', 'on', 'left_on', 'right_on'],
  pivot: ['on', 'index', 'values', 'aggregate_function'],
};

/** Writes the keys of the list parameter `name` of `options` as a list of expressions. */
function keyArguments(context: Context, options: Options, name: string, missing: string): Json {
  const keys = keysOf(options, name);
  if (keys.length === 0) throw unset(missing);
  return {
    many: keys.map((key, index) =>
      expressionArgument(context, key, [`parameter:${name}`, `item:${String(index)}`]),
    ),
  };
}

function stepArguments(context: Context, step: Step, entry: EntryView): Record<string, Json> {
  const own: Record<string, Json> = {};
  const options = (): Record<string, Json> =>
    optionArguments(
      context,
      entry,
      step.options,
      entry.parameters.filter((parameter) => !COVERED[step.verb].includes(parameter.name)),
      (of) => stepItems(step, of),
      [],
      [],
    );
  switch (step.verb) {
    case 'with_columns':
      if (step.alias.trim() === '') throw unset('alias needs a name.');
      own.named_exprs = {
        pairs: [
          [
            columnsOf(step.alias),
            expressionArgument(context, step.value, ['parameter:named_exprs', 'value:0']),
          ],
        ],
      };
      break;
    case 'filter':
      own.predicates = predicates(context, step.condition, ['parameter:predicates']);
      break;
    case 'sort':
      own.by = keyArguments(context, step.options, 'by', 'Pick the fields to sort by.');
      break;
    case 'select':
      own.exprs = keyArguments(context, step.options, 'exprs', 'Pick the fields to keep.');
      break;
    case 'rename': {
      const mapping = entry.parameters.find((parameter) => parameter.name === 'mapping');
      const setting = optionOf(step.options, 'mapping');
      if (mapping === undefined || setting === null || !isComplete(setting)) {
        throw unset('Pick the fields to rename.');
      }
      own.mapping = settingArgument(
        context,
        mapping.kind,
        setting,
        ['parameter:mapping'],
        [],
        mapping,
        [],
      );
      break;
    }
    case 'join':
      if (step.table === '') throw unset('Pick the table to join.');
      own.other = { table: step.table };
      own.left_on = keyArguments(context, step.options, 'left_on', 'Pick the fields to match.');
      own.right_on = keyArguments(context, step.options, 'right_on', 'Pick the fields to match.');
      break;
    case 'pivot': {
      for (const [name, field] of [
        ['on', step.on],
        ['index', step.index],
        ['values', step.values],
      ] as const) {
        if (field === '') throw unset(`Pick the field for ${name}.`);
        own[name] = columnsOf(field);
      }
      const aggregate = optionOf(step.options, 'aggregate_function');
      if (aggregate?.kind !== 'expression' || aggregate.value.kind === 'unset') {
        throw unset('Pick how the values that land in one cell are combined.');
      }
      own.aggregate_function = expressionArgument(context, aggregate.value, [
        'parameter:aggregate_function',
      ]);
      break;
    }
    default:
      return assertNever(step);
  }
  return { ...own, ...options() };
}

/** Writes one step as a document step, or says why it cannot be written. */
function stepJson(catalogue: Catalogue, step: Step): { json: Json; sent: SentStep } | Held {
  const notes: Notes = { marks: new Map(), chains: new Map(), holes: new Map() };
  try {
    const entry = verbEntry(catalogue, step.verb);
    if (entry === null) {
      throw unsupported(
        `The catalogue has no entry LazyFrame.${step.verb}, so this step cannot be run.`,
      );
    }
    const args = stepArguments({ catalogue, notes }, step, entry);
    const json: Json = { id: step.id, call: call(entry.id, args) };
    return {
      json,
      sent: {
        kind: 'sent',
        canonical: JSON.stringify(json),
        marks: notes.marks,
        chains: notes.chains,
        holes: notes.holes,
      },
    };
  } catch (error) {
    if (error instanceof Held) return error;
    throw error;
  }
}

function isList(json: Json): json is readonly Json[] {
  return Array.isArray(json);
}

/** Writes `json` as compact JSON, a float with its fraction or exponent so that the engine reads it as one. */
function jsonText(json: Json, key: string | null = null): string {
  if (typeof json === 'number') {
    const text = JSON.stringify(json);
    return key === 'float' && /^-?\d+$/u.test(text) ? `${text}.0` : text;
  }
  if (json === null || typeof json !== 'object') return JSON.stringify(json);
  if (isList(json)) return `[${json.map((item) => jsonText(item)).join(',')}]`;
  const entries = Object.entries(json).map(
    ([name, value]) => `${JSON.stringify(name)}:${jsonText(value, name)}`,
  );
  return `{${entries.join(',')}}`;
}

/** Writes the editor's `steps` as a step document, and says what became of each step. */
export function translate(catalogue: Catalogue, steps: readonly Step[]): Translation {
  const written: Json[] = [];
  const results: StepTranslation[] = [];
  let blockedBy: string | null = null;
  for (const step of steps) {
    if (blockedBy !== null) {
      results.push({ kind: 'waiting', blockedBy });
      continue;
    }
    const outcome = stepJson(catalogue, step);
    if (outcome instanceof Held) {
      blockedBy = step.id;
      results.push({ kind: 'held', problem: outcome.problem });
    } else {
      written.push(outcome.json);
      results.push(outcome.sent);
    }
  }
  return { text: jsonText({ version: DOCUMENT_VERSION, steps: written }), steps: results };
}
