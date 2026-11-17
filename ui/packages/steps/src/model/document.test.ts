import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import { at } from '../../../host/test-support/tamper';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import documentFixture from '../../../host/fixtures/step-document.json';
import { START } from '../start';
import type { Catalogue } from './catalogue';
import { indexCatalogue } from './catalogue';
import { placeKey, translate, wrongId } from './document';
import type { Options } from './settings';
import { NO_OPTIONS } from './settings';
import type { Condition, Value } from './source';
import type { Step } from './steps';
import { fieldKeys } from './steps';

// The catalogue the host serves, as the editor reads it, and the steps the editor translates.
const catalogue: Catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

/** The document a translation wrote, as the JSON it is. */
function written(steps: readonly Step[]): unknown {
  return JSON.parse(translate(catalogue, steps).text);
}

const col = (name: string): Value => ({ kind: 'column', column: name });
const UNSET_VALUE: Value = { kind: 'unset' };

/** A step that adds a column holding `value`. */
function adding(id: string, value: Value): Step {
  return { id, verb: 'with_columns', alias: id, value, options: NO_OPTIONS };
}

/** The entry and the arguments of the first step of the document `steps` are written as. */
function firstCall(steps: readonly Step[]): {
  readonly entry: unknown;
  readonly arguments: unknown;
} {
  const call = at(written(steps), ['steps', 0, 'call']);
  return { entry: at(call, ['entry']), arguments: at(call, ['arguments']) };
}

/** The arguments the start of the expression of a column added with `value` is given. */
function startOfColumnHolding(value: Value): unknown {
  const arguments_ = firstCall([adding('x', value)]).arguments;
  return at(arguments_, ['named_exprs', 'pairs', 0, 1, 'expression', 'start', 'arguments']);
}

/** The call of a filter over `condition`. */
function filtering(condition: Condition): Step {
  return { id: 'keep', verb: 'filter', condition, options: NO_OPTIONS };
}

const test = (
  id: string,
  column: string,
  comparison: '==' | '!=' | '>' | '>=' | '<' | '<=' | 'is_in',
  value: Value,
): Condition => ({ kind: 'test', id, column, comparison, value });

describe('the steps the editor opens with', () => {
  it('are written as the same document as the engine’s claim-timing sample', () => {
    expect(written(START)).toEqual(documentFixture);
  });

  it('leave no step held back or waiting', () => {
    const { steps } = translate(catalogue, START);

    expect(steps.map((step) => step.kind)).toEqual(START.map(() => 'sent'));
  });
});

describe('a step of the editor', () => {
  it('is written as a call of the entry the catalogue names, with its arguments by parameter', () => {
    const call = firstCall([{ id: 'order', verb: 'sort', options: { by: fieldKeys(['dcn']) } }]);

    expect(call).toEqual({
      entry: 'LazyFrame.sort',
      arguments: {
        by: {
          many: [
            {
              expression: {
                start: { entry: 'polars.col', arguments: { name: { columns: ['dcn'] } } },
                // The document names the links of an expression `then`.
                // oxlint-disable-next-line unicorn/no-thenable
                then: [],
              },
            },
          ],
        },
      },
    });
  });

  it('leaves out a parameter that is set to what Polars does when it is not given', () => {
    const options: Options = {
      left_on: fieldKeys(['a']),
      right_on: fieldKeys(['b']),
      how: { kind: 'word', value: 'inner' },
      coalesce: { kind: 'flag', value: true },
    };

    const call = firstCall([{ id: 'j', verb: 'join', table: 'step_sla', options }]);

    expect(call.arguments).toMatchObject({ coalesce: { flag: true } });
    expect(at(call.arguments, ['how'])).toBeUndefined();
  });

  it('writes the setting of each key of a sort as a list in the order of the keys', () => {
    const options: Options = {
      by: fieldKeys(['dcn', 'timestamp']),
      descending: { kind: 'each', items: { timestamp: { kind: 'flag', value: true } } },
    };

    const call = firstCall([{ id: 'order', verb: 'sort', options }]);

    expect(call.arguments).toMatchObject({
      descending: { many: [{ flag: false }, { flag: true }] },
    });
  });

  it('writes a whole number as an integer and a number with a fraction as a float', () => {
    expect(startOfColumnHolding({ kind: 'literal', value: 3 })).toEqual({ value: { integer: 3 } });
    expect(startOfColumnHolding({ kind: 'literal', value: 1.5 })).toEqual({
      value: { float: 1.5 },
    });
  });
});

describe('what the document cannot hold', () => {
  it('writes a hole where an expression is still to be picked, and still sends the steps after it', () => {
    // The engine stops at the hole, says where it is and does not run what comes after, so the
    // editor sends every step and the engine says why the later ones wait.
    const steps: Step[] = [
      adding('first', col('a')),
      adding('second', UNSET_VALUE),
      adding('third', col('b')),
    ];

    const result = translate(catalogue, steps);

    expect(result.steps.map((step) => step.kind)).toEqual(['sent', 'sent', 'sent']);
    expect(at(JSON.parse(result.text), ['steps'])).toHaveLength(3);
    const second = result.steps[1];
    expect(
      at(JSON.parse(result.text), [
        'steps',
        1,
        'call',
        'arguments',
        'named_exprs',
        'pairs',
        0,
        1,
        'expression',
        'start',
      ]),
    ).toEqual({ hole: null });
    // The hole's place, as the engine reports it, says what is to be picked.
    expect(second).toMatchObject({
      holes: new Map([
        [
          'parameter:named_exprs/value:0/start',
          { cause: 'unset', message: 'Pick a value.', wrong: [] },
        ],
      ]),
    });
  });

  it('holds back a step whose name is empty', () => {
    const step: Step = {
      id: 'x',
      verb: 'with_columns',
      alias: '  ',
      value: col('a'),
      options: NO_OPTIONS,
    };

    expect(translate(catalogue, [step]).steps[0]).toMatchObject({
      kind: 'held',
      problem: { cause: 'unset', message: 'alias needs a name.' },
    });
  });

  it('holds back a comparison with a list, which the engine has no entry for yet, naming the comparison', () => {
    const step = filtering(test('t1', 'dcn', 'is_in', { kind: 'input', name: 'holidays' }));

    expect(translate(catalogue, [step]).steps[0]).toMatchObject({
      kind: 'held',
      problem: { cause: 'unsupported', wrong: [wrongId({ kind: 'test', testId: 't1' })] },
    });
  });

  it('holds back a group of which any one line is enough, and writes one of which every line must be as separate predicates', () => {
    const lines = [
      test('t1', 'a', '==', { kind: 'literal', value: 1 }),
      test('t2', 'b', '>', { kind: 'literal', value: 2 }),
    ];
    const anyOf = filtering({ kind: 'group', id: 'g', every: false, items: lines });
    const allOf = filtering({ kind: 'group', id: 'g', every: true, items: lines });

    expect(translate(catalogue, [anyOf]).steps[0]).toMatchObject({
      kind: 'held',
      problem: { cause: 'unsupported' },
    });
    const call = firstCall([allOf]);
    expect(at(call.arguments, ['predicates', 'many'])).toHaveLength(2);
  });

  it('holds back a None where a value is needed, because the engine has no None value', () => {
    expect(
      translate(catalogue, [adding('x', { kind: 'literal', value: null })]).steps[0],
    ).toMatchObject({
      kind: 'held',
      problem: { cause: 'unsupported' },
    });
  });

  it('holds back a when with more than one case, which the document has no place for', () => {
    const arm = (id: string): { id: string; condition: Condition; returns: Value } => ({
      id,
      condition: test(`t-${id}`, 'a', '==', { kind: 'literal', value: 1 }),
      returns: col('b'),
    });
    const step = adding('x', {
      kind: 'chain',
      id: 'c1',
      chain: {
        source: { kind: 'when', arms: [arm('first'), arm('second')], otherwise: col('c') },
        methods: [],
      },
    });

    expect(translate(catalogue, [step]).steps[0]).toMatchObject({
      kind: 'held',
      problem: { cause: 'unsupported', wrong: [wrongId({ kind: 'start', chainId: 'c1' })] },
    });
  });

  it('writes a hole for a method whose argument is still to be picked, naming the method', () => {
    const step = adding('x', {
      kind: 'chain',
      id: 'c1',
      chain: {
        source: { kind: 'column', column: 'a' },
        methods: [{ id: 'm1', name: 'shift', argument: UNSET_VALUE, options: NO_OPTIONS }],
      },
    });

    const [sent] = translate(catalogue, [step]).steps;

    expect(sent).toMatchObject({
      kind: 'sent',
      holes: new Map([
        [
          'parameter:named_exprs/value:0/link:0/parameter:n/start',
          {
            cause: 'unset',
            message: 'Give shift a value or a field (rows).',
            wrong: [wrongId({ kind: 'method', methodId: 'm1' })],
          },
        ],
      ]),
    });
  });

  it('writes a hole for the field a chain starts from when none is picked yet', () => {
    const step = adding('x', {
      kind: 'chain',
      id: 'c1',
      chain: {
        source: { kind: 'column', column: '' },
        methods: [{ id: 'm1', name: 'cum_count', argument: null, options: NO_OPTIONS }],
      },
    });

    const [sent] = translate(catalogue, [step]).steps;

    expect(sent).toMatchObject({
      holes: new Map([
        [
          'parameter:named_exprs/value:0/start',
          {
            cause: 'unset',
            message: 'Pick the field the chain starts from.',
            wrong: [wrongId({ kind: 'start', chainId: 'c1' })],
          },
        ],
      ]),
    });
  });
});

describe('where the document places the words of the editor', () => {
  const chained = adding('x', {
    kind: 'chain',
    id: 'c1',
    chain: {
      source: { kind: 'column', column: 'a' },
      methods: [
        { id: 'm1', name: 'cum_count', argument: null, options: NO_OPTIONS },
        { id: 'm2', name: 'over', argument: col('b'), options: NO_OPTIONS },
      ],
    },
  });

  it('marks the start of a chain and each of its methods by their places in the step', () => {
    const [sent] = translate(catalogue, [chained]).steps;

    expect(sent?.kind).toBe('sent');
    if (sent?.kind !== 'sent') return;
    const base = ['parameter:named_exprs', 'value:0'];
    expect(sent.marks.get(placeKey([...base, 'start']))).toEqual({ kind: 'start', chainId: 'c1' });
    expect(sent.marks.get(placeKey([...base, 'link:0']))).toEqual({
      kind: 'method',
      methodId: 'm1',
    });
    expect(sent.marks.get(placeKey([...base, 'link:1']))).toEqual({
      kind: 'method',
      methodId: 'm2',
    });
    expect(sent.chains.get(placeKey(base))).toEqual({
      chainId: 'c1',
      whenLinks: 0,
      methodIds: ['m1', 'm2'],
    });
  });

  it('counts the then and the otherwise of a when among the links before its methods', () => {
    const when = adding('x', {
      kind: 'chain',
      id: 'c2',
      chain: {
        source: {
          kind: 'when',
          arms: [
            {
              id: 'arm',
              condition: test('t1', 'a', '==', { kind: 'literal', value: 1 }),
              returns: col('b'),
            },
          ],
          otherwise: { kind: 'literal', value: 0 },
        },
        methods: [{ id: 'm1', name: 'cum_sum', argument: null, options: NO_OPTIONS }],
      },
    });

    const [sent] = translate(catalogue, [when]).steps;

    if (sent?.kind !== 'sent') throw new Error('the step is sent');
    const base = ['parameter:named_exprs', 'value:0'];
    expect(sent.chains.get(placeKey(base))).toEqual({
      chainId: 'c2',
      whenLinks: 2,
      methodIds: ['m1'],
    });
    expect(sent.marks.get(placeKey([...base, 'link:2']))).toEqual({
      kind: 'method',
      methodId: 'm1',
    });
    // The comparison of the when stands inside its start, as the first predicate.
    const predicate = [...base, 'start', 'parameter:predicates', 'item:0'];
    expect(sent.marks.get(placeKey([...predicate, 'link:0']))).toEqual({
      kind: 'test',
      testId: 't1',
    });
  });
});
