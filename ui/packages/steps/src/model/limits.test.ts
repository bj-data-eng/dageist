import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import gridFixture from '../../../host/fixtures/offers-grid.json';
import { allowedAt, decodeGrid } from '../../../host/test-support/grid';
import { indexCatalogue } from './catalogue';
import type { Cell } from './limits';
import { editorLimit, unexpressedParameters, unofferable } from './limits';

// The catalogue and the grid of every place the engine was asked about, as the host writes them.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const grid = decodeGrid(gridFixture);

/** Every place the host wrote, with the entries that may stand there. */
function cells(): Cell[] {
  return grid.contexts.flatMap((context, at) =>
    grid.afters.map((after, kind) => ({
      place: { context, after },
      allowed: allowedAt(grid, at, kind),
    })),
  );
}

const FURTHER_CASE = 'the editor draws one case of a when; a further case follows otherwise';

describe('the places the engine was asked about', () => {
  it('are every context, and for each of them nothing, each dtype in two shapes, and each state of a condition', () => {
    // Six positions of a step, alone and with each of four positions inside an expression.
    expect(grid.contexts).toHaveLength(30);
    // Nothing; twenty-six dtypes, each of one value and of many; four states of a condition.
    expect(grid.afters).toHaveLength(1 + 26 * 2 + 4);
    expect(cells()).toHaveLength(30 * 57);
  });

  it('hold every entry that stands in an expression, and no step', () => {
    expect(grid.entries).toHaveLength(76);
    expect(grid.entries.filter((id) => id.startsWith('LazyFrame.'))).toEqual([]);
    for (const id of grid.entries) {
      expect(catalogue.entries.has(id), id).toBe(true);
    }
  });
});

describe('what the editor cannot offer of what the engine allows', () => {
  it('is a further case of a condition, wherever the engine allows one', () => {
    // The editor draws one `when`, one `then` and one `otherwise`; a further case is a `when`
    // after `otherwise`. The engine allows the links of a further case in every context but the
    // keys of a window in a join key or a pivot's aggregate, 24 of the 30.
    expect(unofferable(catalogue, cells())).toEqual([
      {
        entry: 'ChainedThen.otherwise',
        after: 'condition at chainedThen',
        places: 24,
        reason: FURTHER_CASE,
      },
      {
        entry: 'ChainedThen.when',
        after: 'condition at chainedThen',
        places: 24,
        reason: FURTHER_CASE,
      },
      {
        entry: 'ChainedWhen.then',
        after: 'condition at chainedWhen',
        places: 24,
        reason: FURTHER_CASE,
      },
      { entry: 'Then.when', after: 'condition at then', places: 24, reason: FURTHER_CASE },
    ]);
  });

  it('lists nothing that the editor can draw', () => {
    const listed = new Set(unofferable(catalogue, cells()).map((found) => found.entry));
    for (const id of grid.entries) {
      const entry = catalogue.entries.get(id);
      expect(entry === undefined ? 'no entry' : editorLimit(entry) === null, id).toBe(
        !listed.has(id),
      );
    }
  });

  it('says an entry the catalogue lacks cannot be drawn', () => {
    const lacking = [
      {
        place: { context: { outer: 'select', nested: null }, after: { kind: 'start' } },
        allowed: ['Expr.nothing'],
      },
    ] as const;

    expect(unofferable(catalogue, lacking)).toEqual([
      {
        entry: 'Expr.nothing',
        after: 'start',
        places: 1,
        reason: 'the catalogue has no such entry',
      },
    ]);
  });
});

describe('the parameters of an entry that the editor cannot set', () => {
  it('are those of a function or a condition that it draws only in part', () => {
    const unset = [...catalogue.entries.values()]
      .map((entry): [string, readonly string[]] => [entry.id, unexpressedParameters(entry)])
      .filter(([, names]) => names.length > 0);
    expect(unset).toEqual([
      ['ChainedThen.when', ['constraints']],
      ['Then.when', ['constraints']],
      ['polars.col', ['more_names']],
      ['polars.lit', ['dtype', 'allow_object']],
      ['polars.when', ['constraints']],
    ]);
  });

  it('are none for a method, whose every parameter is of a kind the editor draws', () => {
    const methods = [...catalogue.entries.values()].filter(
      (entry) => entry.applies === 'expression',
    );
    expect(methods.length).toBeGreaterThan(0);
    for (const entry of methods) {
      expect(unexpressedParameters(entry), entry.id).toEqual([]);
    }
  });

  it('include a parameter of a kind the editor has no drawing for, as pairs of pairs', () => {
    const base = catalogue.entries.get('Expr.shift');
    const parameter = base?.parameters[0];
    if (base === undefined || parameter === undefined) throw new Error('no shift');
    const odd = {
      ...base,
      parameters: [
        {
          ...parameter,
          name: 'nested',
          kind: {
            kind: 'pairs',
            key: { kind: 'column' },
            value: { kind: 'pairs', key: { kind: 'column' }, value: { kind: 'flag' } },
          },
        },
        ...base.parameters,
      ],
    } as const;

    expect(unexpressedParameters(odd)).toEqual(['nested']);
  });
});
