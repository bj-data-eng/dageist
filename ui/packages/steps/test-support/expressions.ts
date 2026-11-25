/**
 * @file Test support: builds the expressions of a step document by hand, so that a test can ask
 * the model what the editor does with each kind of start. It is not part of the package's front door.
 */

import type { ArgumentView, CallView, StartView } from '@dageist/host';

import type { ExpressionView } from '../src/model/doc';

/** An expression that starts at `start`, followed by `links`. */
export function chain(start: StartView, links: readonly CallView[] = []): ExpressionView {
  return { kind: 'expression', start, links };
}

/** A call of `entry` with no arguments. */
export function bare(entry: string): CallView {
  return { entry, arguments: [] };
}

/** A call of `polars.col` that reads `names`. */
export function col(...names: string[]): CallView {
  return {
    entry: 'polars.col',
    arguments: [{ parameter: 'name', value: { kind: 'columns', names } }],
  };
}

/** A call of `polars.lit` that holds `value`. */
export function lit(value: ArgumentView): CallView {
  return { entry: 'polars.lit', arguments: [{ parameter: 'value', value }] };
}

/** The start that is a call. */
export function calls(call: CallView): StartView {
  return { kind: 'call', call };
}

/** The start that is a connected input called `name`. */
export function input(name: string): StartView {
  return { kind: 'input', name };
}

/** The start that nothing was picked for. */
export const HOLE_START: StartView = { kind: 'hole' };
