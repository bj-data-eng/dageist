import { describe, expect, it } from 'vitest';

import type { ArgumentView, CallView, DocumentView, StartView } from '@dageist/host';
import { decodeDocumentState } from '@dageist/host';

import documentFixture from '../../../host/fixtures/node-document.json';
import type { ExpressionView, Path } from './doc';
import {
  callAt,
  expressionOf,
  isWithin,
  itemSegment,
  keySegment,
  linkSegment,
  parameterSegment,
  pathKey,
  shapeOf,
  startCall,
  testOf,
  valueSegment,
  whenOf,
} from './doc';

const held = decodeDocumentState(documentFixture);

/** The call of the step `id` of the document-timing steps. */
function callOf(id: string): CallView {
  const found = held.document.steps.find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found.call;
}

/** The expression the first column of a `with_columns` step is made from. */
function columnOf(id: string): ExpressionView {
  const pairs = callOf(id).arguments.find((given) => given.parameter === 'named_exprs')?.value;
  const value = pairs?.kind === 'pairs' ? pairs.entries[0]?.value : undefined;
  const expression = expressionOf(value);
  if (expression === null) throw new Error(`step ${id} makes no expression`);
  return expression;
}

/** An expression that starts at `start`, followed by `links`. */
function chain(start: StartView, links: CallView[] = []): ExpressionView {
  return { kind: 'expression', start, links };
}

/** A call of `polars.col` that reads `names`. */
function col(...names: string[]): CallView {
  return {
    entry: 'polars.col',
    arguments: [{ parameter: 'name', value: { kind: 'columns', names } }],
  };
}

/** A call of `polars.lit` that holds `value`. */
function lit(value: ArgumentView): CallView {
  return { entry: 'polars.lit', arguments: [{ parameter: 'value', value }] };
}

describe('what an expression is, as the editor draws it', () => {
  it('is unset for a hole with nothing after it', () => {
    expect(shapeOf(chain({ kind: 'hole' }))).toEqual({ kind: 'unset' });
  });

  it('is a column for the one field read alone', () => {
    expect(shapeOf(chain({ kind: 'call', call: col('document_id') }))).toEqual({
      kind: 'column',
      name: 'document_id',
    });
  });

  it('is a chain when more than one field is read, or a field is followed by a method', () => {
    expect(shapeOf(chain({ kind: 'call', call: col('a', 'b') }))).toEqual({ kind: 'chain' });
    expect(
      shapeOf(
        chain({ kind: 'call', call: col('a') }, [{ entry: 'Expr.cum_count', arguments: [] }]),
      ),
    ).toEqual({ kind: 'chain' });
  });

  it('is an input for a connected input alone', () => {
    expect(shapeOf(chain({ kind: 'input', name: 'holidays' }))).toEqual({
      kind: 'input',
      name: 'holidays',
    });
  });

  it('is a literal, with the words the engine shows it as, for a number or text alone', () => {
    expect(shapeOf(chain({ kind: 'call', call: lit({ kind: 'integer', shown: '1' }) }))).toEqual({
      kind: 'literal',
      shown: '1',
      text: false,
    });
    expect(shapeOf(chain({ kind: 'call', call: lit({ kind: 'float', shown: '1.5' }) }))).toEqual({
      kind: 'literal',
      shown: '1.5',
      text: false,
    });
    expect(
      shapeOf(chain({ kind: 'call', call: lit({ kind: 'string', value: 'Company A' }) })),
    ).toEqual({ kind: 'literal', shown: 'Company A', text: true });
  });

  it('is a chain for any other literal, such as a flag, or one with another argument', () => {
    expect(shapeOf(chain({ kind: 'call', call: lit({ kind: 'flag', value: true }) }))).toEqual({
      kind: 'chain',
    });
    expect(
      shapeOf(
        chain({
          kind: 'call',
          call: {
            entry: 'polars.lit',
            arguments: [
              { parameter: 'value', value: { kind: 'integer', shown: '1' } },
              { parameter: 'dtype', value: { kind: 'dtype', name: 'Int8' } },
            ],
          },
        }),
      ),
    ).toEqual({ kind: 'chain' });
  });

  it('is read off the document-timing steps the same way', () => {
    expect(shapeOf(columnOf('received_at'))).toEqual({ kind: 'column', name: 'timestamp' });
    expect(shapeOf(columnOf('step_index'))).toEqual({ kind: 'chain' });
  });
});

describe('the field a call reads', () => {
  it('is none for a call that is not polars.col', () => {
    expect(startCall({ kind: 'input', name: 'x' })).toBeNull();
    expect(startCall({ kind: 'hole' })).toBeNull();
  });
});

describe('a condition drawn as when, then and otherwise', () => {
  it('is the layout of a when with its then and otherwise', () => {
    const layout = whenOf(columnOf('a_days'));

    expect(layout?.call.entry).toBe('polars.when');
    expect(layout?.before).toBe(2);
    expect(layout?.predicates?.kind).toBe('many');
  });

  it('is none for a chain that does not start from when, or lacks the otherwise', () => {
    expect(whenOf(columnOf('step_index'))).toBeNull();
    const when = columnOf('a_days');
    expect(whenOf({ ...when, links: when.links.slice(0, 1) })).toBeNull();
    // A further case: `then` followed by another `when`-like link is not the one case drawn.
    const [then] = when.links;
    if (then === undefined) throw new Error('no then');
    expect(
      whenOf({ ...when, links: [then, { entry: 'Then.when', arguments: [] }, ...when.links] }),
    ).toBeNull();
  });
});

describe('a test of a condition', () => {
  const comparisons = new Set(['Expr.eq', 'Expr.le']);
  const other: ArgumentView = { kind: 'integer', shown: '3' };

  it('is a field and one comparison, with what it is compared with', () => {
    const comparison: CallView = {
      entry: 'Expr.eq',
      arguments: [{ parameter: 'other', value: other }],
    };

    expect(testOf(chain({ kind: 'call', call: col('owner') }, [comparison]), comparisons)).toEqual({
      column: 'owner',
      comparison,
      other,
    });
  });

  it('is still a test where the field is yet to be picked', () => {
    const comparison: CallView = { entry: 'Expr.le', arguments: [] };

    expect(testOf(chain({ kind: 'hole' }, [comparison]), comparisons)).toEqual({
      column: null,
      comparison,
      other: undefined,
    });
  });

  it('is not a test when more than one link follows the start or the link is not a comparison', () => {
    const eq: CallView = { entry: 'Expr.eq', arguments: [] };
    const start: StartView = { kind: 'call', call: col('owner') };

    expect(testOf(chain(start, [eq, eq]), comparisons)).toBeNull();
    expect(testOf(chain(start, []), comparisons)).toBeNull();
    expect(
      testOf(chain(start, [{ entry: 'Expr.cum_sum', arguments: [] }]), comparisons),
    ).toBeNull();
  });

  it('is not a test when the comparison holds an argument other than `other`, or the start is not a field', () => {
    const odd: CallView = {
      entry: 'Expr.eq',
      arguments: [{ parameter: 'mystery', value: { kind: 'integer', shown: '1' } }],
    };
    const eq: CallView = { entry: 'Expr.eq', arguments: [] };

    expect(testOf(chain({ kind: 'call', call: col('a') }, [odd]), comparisons)).toBeNull();
    expect(testOf(chain({ kind: 'input', name: 'x' }, [eq]), comparisons)).toBeNull();
    expect(testOf(chain({ kind: 'call', call: col('a', 'b') }, [eq]), comparisons)).toBeNull();
    expect(
      testOf(
        chain({ kind: 'call', call: lit({ kind: 'integer', shown: '1' }) }, [eq]),
        comparisons,
      ),
    ).toBeNull();
  });
});

describe('a way within another', () => {
  const by: Path = [parameterSegment('by'), itemSegment(1)];

  it('is within itself and within any way it goes further down from', () => {
    expect(isWithin(by, by)).toBe(true);
    expect(isWithin(by, [parameterSegment('by')])).toBe(true);
    expect(isWithin(by, [])).toBe(true);
  });

  it('is not within a way that goes further, or a different one', () => {
    expect(isWithin([parameterSegment('by')], by)).toBe(false);
    expect(isWithin(by, [parameterSegment('by'), itemSegment(0)])).toBe(false);
    expect(isWithin(by, [parameterSegment('other')])).toBe(false);
  });

  it('tells a key from a value of the same index, and an item from a link', () => {
    expect(pathKey([keySegment(0)])).not.toBe(pathKey([valueSegment(0)]));
    expect(pathKey([itemSegment(0)])).not.toBe(pathKey([linkSegment(0)]));
    expect(isWithin([keySegment(0)], [valueSegment(0)])).toBe(false);
  });
});

describe('the call a way reaches in a step', () => {
  const document: DocumentView = held.document;
  const stepIndex = document.steps.find((step) => step.id === 'step_index')?.call;
  const column: Path = [parameterSegment('named_exprs'), valueSegment(0)];

  it('is the step’s own call for the empty way', () => {
    if (stepIndex === undefined) throw new Error('no step');
    expect(callAt(stepIndex, [])).toBe(stepIndex);
  });

  it('is a link of a chain, or the call a chain starts with', () => {
    if (stepIndex === undefined) throw new Error('no step');

    expect(callAt(stepIndex, [...column, linkSegment(0)])?.entry).toBe('Expr.cum_count');
    expect(callAt(stepIndex, [...column, linkSegment(1)])?.entry).toBe('Expr.over');
    expect(callAt(stepIndex, [...column, { kind: 'start' }])?.entry).toBe('polars.col');
  });

  it('is a call inside an argument of a link', () => {
    if (stepIndex === undefined) throw new Error('no step');
    const inside: Path = [
      ...column,
      linkSegment(1),
      parameterSegment('partition_by'),
      itemSegment(0),
      { kind: 'start' },
    ];

    expect(callAt(stepIndex, inside)?.entry).toBe('polars.col');
  });

  it('is nothing where the document has nothing, or what is there is not a call', () => {
    if (stepIndex === undefined) throw new Error('no step');

    expect(callAt(stepIndex, [parameterSegment('nope')])).toBeNull();
    expect(callAt(stepIndex, [...column, linkSegment(9)])).toBeNull();
    // The argument itself is not a call.
    expect(callAt(stepIndex, column)).toBeNull();
    expect(callAt(stepIndex, [parameterSegment('named_exprs'), itemSegment(0)])).toBeNull();
  });
});
