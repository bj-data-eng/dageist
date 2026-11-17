import { describe, expect, it } from 'vitest';

import { decodeCatalogue, decodeNode } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import nodeFixture from '../../../host/fixtures/node-inputs.json';
import { indexCatalogue } from './catalogue';
import { translate } from './document';
import { startingArgument, swappedArgument } from './expr';
import { tablesOf } from './inputs';
import { defaultStep } from './steps';

const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const node = decodeNode(nodeFixture);
const tables = tablesOf(node);

describe('a new step', () => {
  it('is a join on the first connected table, whose keys are still to be picked', () => {
    // The fields are not matched by their dtype: the engine says whether two keys fit, and the
    // step says what is missing until they are picked.
    const step = defaultStep('join', node.source.columns, tables);

    expect(step).toMatchObject({
      verb: 'join',
      table: 'step_names',
      options: {
        left_on: { kind: 'list', items: [{ kind: 'unset' }] },
        right_on: { kind: 'list', items: [{ kind: 'unset' }] },
        how: { kind: 'word', value: 'left' },
      },
    });
    const [held] = translate(catalogue, [step]).steps;
    expect(held).toMatchObject({ kind: 'held', problem: { cause: 'unset' } });
  });

  it('is a pivot whose three fields are still to be picked, over the first connected table', () => {
    const step = defaultStep('pivot', node.source.columns, tables);

    expect(step).toMatchObject({
      verb: 'pivot',
      on: '',
      index: '',
      values: '',
      options: { on_columns: { kind: 'word', value: 'step_names' } },
    });
    const [held] = translate(catalogue, [step]).steps;
    expect(held).toMatchObject({
      kind: 'held',
      problem: { cause: 'unset', message: 'Pick the field for on.' },
    });
  });

  it('is a join with no table at all when none is connected', () => {
    expect(defaultStep('join', node.source.columns, [])).toMatchObject({ table: '' });
    expect(defaultStep('pivot', node.source.columns, [])).toMatchObject({
      options: { on_columns: { kind: 'unset' } },
    });
  });

  it('keeps every field for a select, in the order of the rows', () => {
    const step = defaultStep('select', node.source.columns, tables);

    expect(step).toMatchObject({ verb: 'select' });
    expect(step.verb === 'select' ? step.options.exprs : null).toMatchObject({
      kind: 'list',
      items: node.source.columns.map((column) => ({
        kind: 'expression',
        value: { kind: 'column', column: column.name },
      })),
    });
  });
});

describe('the argument of a method that is added', () => {
  it('is the default the engine allowed, a number typed in', () => {
    expect(
      startingArgument(catalogue, 'shift', {
        parameter: 'n',
        literal: { kind: 'integer', value: 1 },
      }),
    ).toEqual({ kind: 'literal', value: 1 });
  });

  it('is still to be picked where the engine allowed none, and none for a method that takes none', () => {
    expect(startingArgument(catalogue, 'shift', null)).toEqual({ kind: 'unset' });
    // An argument the engine allowed for another parameter is not this one's.
    expect(
      startingArgument(catalogue, 'shift', {
        parameter: 'fill_value',
        literal: { kind: 'text', value: 'x' },
      }),
    ).toEqual({ kind: 'unset' });
    expect(startingArgument(catalogue, 'is_null', null)).toBeNull();
    expect(startingArgument(catalogue, 'nothing', null)).toBeNull();
  });

  it('is kept when a method is swapped, and the new method’s own where there was none', () => {
    const kept = { kind: 'column', column: 'a' } as const;
    expect(swappedArgument(catalogue, kept, 'sub', null)).toBe(kept);
    expect(swappedArgument(catalogue, { kind: 'unset' }, 'shift', null)).toEqual({ kind: 'unset' });
    expect(
      swappedArgument(catalogue, null, 'shift', {
        parameter: 'n',
        literal: { kind: 'integer', value: 1 },
      }),
    ).toEqual({ kind: 'literal', value: 1 });
    // A method that takes no argument has none to keep.
    expect(swappedArgument(catalogue, kept, 'is_null', null)).toBeNull();
  });
});
