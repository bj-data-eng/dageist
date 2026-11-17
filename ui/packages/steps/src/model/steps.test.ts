import { describe, expect, it } from 'vitest';

import { decodeCatalogue, decodeNode } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import nodeFixture from '../../../host/fixtures/node-inputs.json';
import { indexCatalogue } from './catalogue';
import { translate } from './document';
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
});
