import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import { indexCatalogue } from './catalogue';
import { startingArgument, swappedArgument } from './expr';
import { expressionSetting } from './settings';
import { stepItems } from './steps';
import type { Step } from './steps';

const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));

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

describe('the keys of a sort that each have a setting of their own', () => {
  it('are told apart by the name of the field, or by their place where a key is not a field', () => {
    const sort: Step = {
      id: 'sort',
      verb: 'sort',
      options: {
        by: {
          kind: 'list',
          items: [
            expressionSetting({ kind: 'column', column: 'dcn' }),
            { kind: 'unset' },
            {
              kind: 'expression',
              value: { kind: 'literal', value: 3 },
            },
          ],
        },
      },
    };

    // The key still to be picked is not one yet; the one after it is the third in the list.
    expect(stepItems(sort, 'by')).toEqual(['dcn', 'key 3']);
    expect(stepItems(sort, 'other')).toEqual([]);
  });
});
