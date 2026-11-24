/**
 * @file The document the host holds, as the editor reads it: the places in it, and what an
 * expression of it is as far as the editor draws it. The editor never writes a document. It draws
 * the document it is sent and says what the person did as an edit, which the host's engine makes;
 * what is here only finds things in what was sent and names where they are.
 */

import type {
  ArgumentView,
  CallView,
  DocumentStepView,
  PlaceView,
  SegmentView,
  StartView,
} from '@dageist/host';

/** The way from a step to a thing inside it, outermost first. */
export type Path = readonly SegmentView[];

/** The way to the call of a step itself. */
export const NO_PATH: Path = [];

/** The segment that is the argument for the parameter `name`. */
export function parameterSegment(name: string): SegmentView {
  return { kind: 'parameter', name };
}

/** The segment that is the item at `index` of a list. */
export function itemSegment(index: number): SegmentView {
  return { kind: 'item', index };
}

/** The segment that is the key of the entry at `index` of pairs. */
export function keySegment(index: number): SegmentView {
  return { kind: 'key', index };
}

/** The segment that is the value of the entry at `index` of pairs. */
export function valueSegment(index: number): SegmentView {
  return { kind: 'value', index };
}

/** The segment that is the start of an expression. */
export const START_SEGMENT: SegmentView = { kind: 'start' };

/** The segment that is the link at `index` of an expression. */
export function linkSegment(index: number): SegmentView {
  return { kind: 'link', index };
}

/** Returns `path` followed by `segments`. */
export function under(path: Path, ...segments: readonly SegmentView[]): Path {
  return [...path, ...segments];
}

/** Returns one word for a segment, so that a way can be a key. */
function word(segment: SegmentView): string {
  switch (segment.kind) {
    case 'parameter':
      return `parameter:${segment.name}`;
    case 'item':
    case 'key':
    case 'value':
    case 'link':
      return `${segment.kind}:${String(segment.index)}`;
    case 'start':
      return 'start';
    default:
      return segment satisfies never;
  }
}

/** Returns what tells a way apart from the others in a step. */
export function pathKey(path: Path): string {
  return path.map(word).join('/');
}

/** Returns whether `path` is `prefix` or goes further down from it. */
export function isWithin(path: Path, prefix: Path): boolean {
  return (
    prefix.length <= path.length &&
    prefix.every((segment, at) => word(segment) === word(path[at] ?? START_SEGMENT))
  );
}

/** The step the editor is drawing: its place in the document and its name. */
export interface StepAt {
  readonly index: number;
  readonly id: string;
}

/** Returns the place of `path` in the step `step`, as the host names places. */
export function placeOf(step: StepAt, path: Path): PlaceView {
  return { stepIndex: step.index, stepId: step.id, path };
}

/** Returns the argument given for `parameter` of `call`, or `undefined` when it is not given. */
export function givenOf(call: CallView, parameter: string): ArgumentView | undefined {
  return call.arguments.find((given) => given.parameter === parameter)?.value;
}

/** Returns the step of `steps` called `id`, with its place in the list. */
export function stepNamed(
  steps: readonly DocumentStepView[],
  id: string,
): { readonly step: DocumentStepView; readonly index: number } | null {
  const index = steps.findIndex((step) => step.id === id);
  const step = steps[index];
  return step === undefined ? null : { step, index };
}

/** An expression, as the document holds it. */
export type ExpressionView = Extract<ArgumentView, { readonly kind: 'expression' }>;

/** Returns `argument` as an expression, or `null` when it is any other kind of argument. */
export function expressionOf(argument: ArgumentView | undefined): ExpressionView | null {
  return argument?.kind === 'expression' ? argument : null;
}

/** What an expression is, as far as the editor draws it as a word and not as a chain. */
export type Shape =
  /** Nothing is picked: its start is a hole and nothing follows. */
  | { readonly kind: 'unset' }
  /** A field alone. */
  | { readonly kind: 'column'; readonly name: string }
  /** A number or text typed in, alone, with the words the engine shows it as. */
  | { readonly kind: 'literal'; readonly shown: string; readonly text: boolean }
  /** A connected input alone. */
  | { readonly kind: 'input'; readonly name: string }
  /** Anything else, which is drawn as a chain. */
  | { readonly kind: 'chain' };

/** Returns the one name a call of `polars.col` reads, or `null` when it is not one. */
export function fieldNameOf(call: CallView): string | null {
  if (call.entry !== 'polars.col') return null;
  const [only, ...rest] = call.arguments;
  if (rest.length > 0 || only?.parameter !== 'name' || only.value.kind !== 'columns') return null;
  const [name, ...more] = only.value.names;
  return name !== undefined && more.length === 0 ? name : null;
}

/** Returns what an expression is, in the editor's terms. */
export function shapeOf(expression: ExpressionView): Shape {
  if (expression.links.length > 0) return { kind: 'chain' };
  const { start } = expression;
  switch (start.kind) {
    case 'hole':
      return { kind: 'unset' };
    case 'input':
      return { kind: 'input', name: start.name };
    case 'call': {
      const name = fieldNameOf(start.call);
      if (name !== null) return { kind: 'column', name };
      if (start.call.entry === 'polars.lit' && start.call.arguments.length === 1) {
        const value = start.call.arguments[0]?.value;
        if (value?.kind === 'integer' || value?.kind === 'float') {
          return { kind: 'literal', shown: value.shown, text: false };
        }
        if (value?.kind === 'string') return { kind: 'literal', shown: value.value, text: true };
      }
      return { kind: 'chain' };
    }
    default:
      return start satisfies never;
  }
}

/** The calls that stand where an expression starts. */
export function startCall(start: StartView): CallView | null {
  return start.kind === 'call' ? start.call : null;
}

/** The links of a condition that the editor draws as `then` and `otherwise`. */
const THEN = 'When.then';
const OTHERWISE = 'Then.otherwise';

/** An expression that starts from `when`, with its one case drawn as `then` and `otherwise`. */
export interface WhenLayout {
  /** The `when` call. */
  readonly call: CallView;
  /** The conditions, which must all hold: the list `predicates` of the call, or `null` where it is not a list. */
  readonly predicates: ArgumentView | undefined;
  /** How many links come before the methods: the `then` and the `otherwise`. */
  readonly before: number;
}

/**
 * Returns the layout of an expression that starts from `when`, then, otherwise, and perhaps
 * methods, which is the one case the editor draws; `null` for any other expression. A further
 * case is not drawn as one, and is drawn as the methods it is made of.
 */
export function whenOf(expression: ExpressionView): WhenLayout | null {
  const call = startCall(expression.start);
  if (call?.entry !== 'polars.when') return null;
  const [then, otherwise] = expression.links;
  if (then?.entry !== THEN || otherwise?.entry !== OTHERWISE) return null;
  return { call, predicates: givenOf(call, 'predicates'), before: 2 };
}

/** Returns whether `call` is the start `polars.element`. */
export function isElement(call: CallView): boolean {
  return call.entry === 'polars.element';
}

/** What a comparison of a test is made of. */
export interface TestView {
  /** The field it reads, or `null` where it is still to be picked. */
  readonly column: string | null;
  /** The call that compares: its entry is one of the comparisons the catalogue lists. */
  readonly comparison: CallView;
  /** What it is compared with, or `undefined` where nothing is given. */
  readonly other: ArgumentView | undefined;
}

/**
 * Returns the test an expression is, when it is a field and one comparison of the entries
 * `comparisons`, as a filter's lines are; `null` for any other expression.
 */
export function testOf(
  expression: ExpressionView,
  comparisons: ReadonlySet<string>,
): TestView | null {
  const [comparison, ...rest] = expression.links;
  if (comparison === undefined || rest.length > 0 || !comparisons.has(comparison.entry)) {
    return null;
  }
  if (comparison.arguments.some((given) => given.parameter !== 'other')) return null;
  const { start } = expression;
  if (start.kind === 'input') return null;
  const column = start.kind === 'hole' ? null : fieldNameOf(start.call);
  if (start.kind === 'call' && column === null) return null;
  return { column, comparison, other: givenOf(comparison, 'other') };
}
