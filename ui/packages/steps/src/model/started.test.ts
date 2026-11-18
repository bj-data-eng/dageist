import { describe, expect, it } from 'vitest';

import type { SettingView, StepStartView } from '@dageist/host';
import { decodeCatalogue, decodeNode, decodeStepStarts } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import nodeFixture from '../../../host/fixtures/node-inputs.json';
import startsFixture from '../../../host/fixtures/step-starts.json';
import { indexCatalogue } from './catalogue';
import { translate } from './document';
import { defaultOf, settingOf, startOf, startedStep } from './started';
import type { Step, Verb } from './steps';

// The catalogue, the node and what each step starts with, as the host wrote them.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const node = decodeNode(nodeFixture);
const starts = decodeStepStarts(startsFixture).starts;

function startFor(verb: Verb): StepStartView {
  const found = starts.find((start) => start.entry === `LazyFrame.${verb}`);
  if (found === undefined) throw new Error(`no start for ${verb}`);
  return found;
}

function parameterOf(entry: string, name: string) {
  const found = catalogue.entries
    .get(entry)
    ?.parameters.find((parameter) => parameter.name === name);
  if (found === undefined) throw new Error(`no parameter ${name} of ${entry}`);
  return found;
}

describe('a setting the engine reports', () => {
  it('is read as the setting of the parameter that holds it', () => {
    expect(settingOf({ kind: 'unset' })).toEqual({ kind: 'unset' });
    expect(settingOf({ kind: 'flag', value: true })).toEqual({ kind: 'flag', value: true });
    // A number or text comes with the words the engine shows it as.
    expect(settingOf({ kind: 'integer', value: 3, shown: '3' })).toEqual({
      kind: 'number',
      value: 3,
      shown: '3',
    });
    // A number the engine read with a fraction stays one though it is whole.
    expect(settingOf({ kind: 'float', value: 2, shown: '2.0' })).toEqual({
      kind: 'number',
      value: 2,
      float: true,
      shown: '2.0',
    });
    expect(settingOf({ kind: 'text', value: '_x', shown: '_x' })).toEqual({
      kind: 'text',
      value: '_x',
      shown: '_x',
    });
    expect(settingOf({ kind: 'word', value: 'left' })).toEqual({ kind: 'word', value: 'left' });
    expect(settingOf({ kind: 'field', column: 'dcn' })).toEqual({
      kind: 'expression',
      value: { kind: 'column', column: 'dcn' },
    });
    expect(settingOf({ kind: 'list', items: [{ kind: 'unset' }] })).toEqual({
      kind: 'list',
      items: [{ kind: 'unset' }],
    });
  });

  it('keeps the entries of pairs, each with an identifier of its own that stays with it', () => {
    const pairs = settingOf({
      kind: 'pairs',
      entries: [
        { key: { kind: 'text', value: 'a', shown: 'a' }, value: { kind: 'unset' } },
        { key: { kind: 'text', value: 'b', shown: 'b' }, value: { kind: 'unset' } },
      ],
    });

    if (pairs.kind !== 'pairs') throw new Error('not pairs');
    expect(pairs.entries.map((entry) => [entry.key, entry.value])).toEqual([
      [{ kind: 'text', value: 'a', shown: 'a' }, { kind: 'unset' }],
      [{ kind: 'text', value: 'b', shown: 'b' }, { kind: 'unset' }],
    ]);
    expect(new Set(pairs.entries.map((entry) => entry.id)).size).toBe(2);
  });

  it('reads an expression made of element and a method as that chain', () => {
    const setting = settingOf({ kind: 'element', method: 'first' });

    expect(setting).toMatchObject({
      kind: 'expression',
      value: {
        kind: 'chain',
        chain: { source: { kind: 'element' }, methods: [{ name: 'first', argument: null }] },
      },
    });
  });
});

describe('what a parameter holds and is first set to', () => {
  it('are what the engine reports: Polars’ default as a setting, and something that differs', () => {
    expect(defaultOf(parameterOf('LazyFrame.join', 'how'))).toEqual({
      kind: 'word',
      value: 'inner',
    });
    expect(defaultOf(parameterOf('LazyFrame.sort', 'descending'))).toEqual({
      kind: 'flag',
      value: false,
    });
    expect(defaultOf(parameterOf('LazyFrame.join', 'suffix'))).toEqual({
      kind: 'text',
      value: '_right',
      shown: '_right',
    });
    // Where Polars decides, or the parameter must be given, it holds nothing.
    expect(defaultOf(parameterOf('LazyFrame.join', 'coalesce'))).toBeNull();
    expect(startOf(parameterOf('LazyFrame.join', 'how'))).toEqual({ kind: 'word', value: 'left' });
    expect(startOf(parameterOf('LazyFrame.sort', 'descending'))).toEqual({
      kind: 'flag',
      value: true,
    });
    expect(startOf(parameterOf('LazyFrame.join', 'left_on'))).toEqual({
      kind: 'list',
      items: [{ kind: 'unset' }],
    });
    expect(startOf(parameterOf('LazyFrame.rename', 'mapping'))).toEqual({
      kind: 'pairs',
      entries: [],
    });
    expect(startOf(parameterOf('Expr.shift', 'n'))).toEqual({ kind: 'unset' });
  });
});

/** The start of `entry` with the engine's table for `parameter` taken away, as it gives where none is connected. */
const noTable = (entry: string, parameter: string): StepStartView => {
  const start = starts.find((candidate) => candidate.entry === entry);
  if (start === undefined) throw new Error(`no start for ${entry}`);
  return {
    entry,
    arguments: start.arguments.map((argument) =>
      argument.parameter === parameter ? { parameter, setting: { kind: 'unset' } } : argument,
    ),
  };
};

/** The start of a new column that holds `setting`. */
const withColumns = (setting: SettingView): StepStartView => ({
  entry: 'LazyFrame.with_columns',
  arguments: [
    {
      parameter: 'named_exprs',
      setting: {
        kind: 'pairs',
        entries: [{ key: { kind: 'text', value: 'x', shown: 'x' }, value: setting }],
      },
    },
  ],
});

/** The start of a filter whose first condition is `setting`. */
const filtering = (setting: SettingView): StepStartView => ({
  entry: 'LazyFrame.filter',
  arguments: [{ parameter: 'predicates', setting: { kind: 'list', items: [setting] } }],
});

describe('a new step', () => {
  it('is a join on the first connected table, whose keys are still to be picked', () => {
    // The fields are not matched by their dtype: the engine says whether two keys fit, and the
    // step says what is missing until they are picked.
    const step = startedStep('join', startFor('join'));

    expect(step).toMatchObject({
      verb: 'join',
      table: 'step_names',
      options: {
        left_on: { kind: 'list', items: [{ kind: 'unset' }] },
        right_on: { kind: 'list', items: [{ kind: 'unset' }] },
        how: { kind: 'word', value: 'left' },
      },
    });
    // The keys are holes in the document, so the engine says the step is incomplete.
    const [sent] = translate(catalogue, [step]).steps;
    expect(sent).toMatchObject({ kind: 'sent' });
  });

  it('is a pivot whose three fields are still to be picked, over the first connected table', () => {
    const step = startedStep('pivot', startFor('pivot'));

    expect(step).toMatchObject({
      verb: 'pivot',
      on: '',
      index: '',
      values: '',
      options: {
        on_columns: { kind: 'word', value: 'step_names' },
        // Each cell holds one row, so the first value is the only one.
        aggregate_function: {
          kind: 'expression',
          value: {
            kind: 'chain',
            chain: { source: { kind: 'element' }, methods: [{ name: 'first' }] },
          },
        },
      },
    });
    const [held] = translate(catalogue, [step]).steps;
    expect(held).toMatchObject({
      kind: 'held',
      problem: { cause: 'unset', message: 'Pick the field for on.' },
    });
  });

  it('is a join or a pivot with no table at all when the engine gave none', () => {
    expect(startedStep('join', noTable('LazyFrame.join', 'other'))).toMatchObject({ table: '' });
    expect(startedStep('pivot', noTable('LazyFrame.pivot', 'on_columns'))).toMatchObject({
      options: { on_columns: { kind: 'unset' } },
    });
  });

  it('keeps every field for a select, in the order of the rows', () => {
    const step: Step = startedStep('select', startFor('select'));

    expect(step).toMatchObject({ verb: 'select' });
    expect(step.verb === 'select' ? step.options.exprs : null).toMatchObject({
      kind: 'list',
      items: node.source.columns.map((column) => ({
        kind: 'expression',
        value: { kind: 'column', column: column.name },
      })),
    });
  });

  it('is a sort whose first key is still to be picked, and a rename with no pairs yet', () => {
    expect(startedStep('sort', startFor('sort'))).toMatchObject({
      verb: 'sort',
      options: { by: { kind: 'list', items: [{ kind: 'unset' }] } },
    });
    expect(startedStep('rename', startFor('rename'))).toMatchObject({
      verb: 'rename',
      options: { mapping: { kind: 'pairs', entries: [] } },
    });
  });

  it('is a column called new_column with what it holds still to be picked', () => {
    expect(startedStep('with_columns', startFor('with_columns'))).toMatchObject({
      verb: 'with_columns',
      alias: 'new_column',
      value: { kind: 'unset' },
      options: {},
    });
  });

  it('is a filter whose comparison, for equality, has no field or value yet', () => {
    expect(startedStep('filter', startFor('filter'))).toMatchObject({
      verb: 'filter',
      condition: { kind: 'test', column: '', comparison: 'Expr.eq', value: { kind: 'unset' } },
    });
  });

  it('is a filter that compares as the engine says, with the field and value it gives', () => {
    const start: StepStartView = {
      entry: 'LazyFrame.filter',
      arguments: [
        {
          parameter: 'predicates',
          setting: {
            kind: 'list',
            items: [
              {
                kind: 'comparison',
                entry: 'Expr.gt',
                field: { kind: 'field', column: 'dcn' },
                other: { kind: 'field', column: 'owner' },
              },
            ],
          },
        },
      ],
    };

    expect(startedStep('filter', start)).toMatchObject({
      condition: {
        kind: 'test',
        column: 'dcn',
        comparison: 'Expr.gt',
        value: { kind: 'column', column: 'owner' },
      },
    });
  });

  it('is refused where the engine says something the editor cannot hold, and not made into a hole', () => {
    const join: StepStartView = {
      entry: 'LazyFrame.join',
      arguments: [{ parameter: 'other', setting: { kind: 'flag', value: true } }],
    };

    // What a new column holds is an expression; a flag is not one.
    expect(() => startedStep('with_columns', withColumns({ kind: 'flag', value: true }))).toThrow(
      /flag/,
    );
    // A table is a name.
    expect(() => startedStep('join', join)).toThrow(/flag/);
    // A condition is a comparison.
    expect(() => startedStep('filter', filtering({ kind: 'flag', value: true }))).toThrow(/flag/);
    expect(() =>
      startedStep(
        'filter',
        filtering({
          kind: 'comparison',
          entry: 'Expr.nothing',
          field: { kind: 'unset' },
          other: { kind: 'unset' },
        }),
      ),
    ).toThrow(/Expr.nothing/);
    // A comparison is not the setting of a parameter.
    expect(() =>
      settingOf({
        kind: 'comparison',
        entry: 'Expr.eq',
        field: { kind: 'unset' },
        other: { kind: 'unset' },
      }),
    ).toThrow(/comparison/);
  });

  it('is a step of its own each time, with an identifier that stays with it', () => {
    const first = startedStep('sort', startFor('sort'));
    const second = startedStep('sort', startFor('sort'));

    expect(first.id).not.toBe(second.id);
  });
});
