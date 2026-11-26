import { describe, expect, it } from 'vitest';

import type { EntryView } from '@dageist/host';
import { decodeOffers, decodeOptions } from '@dageist/host';

import offersConditionFixture from '../../../host/fixtures/offers-condition.json';
import offersStartFixture from '../../../host/fixtures/offers-start.json';
import offersValueFixture from '../../../host/fixtures/offers-value.json';
import optionsFixture from '../../../host/fixtures/options.json';
import { allowedAt } from '../../../host/test-support/grid';
import {
  FUNCTIONS,
  STARTS,
  catalogue,
  compare,
  entryOf,
  findGaps,
  grid,
} from '../../test-support/grid-gaps';
import type { Start } from '../../test-support/grid-gaps';
import { HOLE_START, chain } from '../../test-support/expressions';
import { isDrawn } from '../../test-support/places';
import { sorted } from '../../test-support/sorted';
import { stepOptionParameters } from './catalogue';
import { chainBodyOf, listsOf, startWordOf } from './words';
import type { Catalogue } from './catalogue';
import { settingOf } from './controls';
import type { Gap } from './limits';
import { GAPS } from './limits';

// For every place the engine was asked about, every start that leads to it and every entry the
// engine allows there, this runs the model's functions, which the components call, and asks two
// things: whether the entry can be picked or added at that place, and whether each of its
// parameters gets a control. What it finds is compared with the declared gaps. The expectations
// are written from what the editor draws and what the grid means, not from what it prints.
/** The run over the grid the host wrote. */
const found = findGaps(grid, catalogue, STARTS, grid.contexts);

describe('the places the engine was asked about', () => {
  it('are every context, and for each of them nothing, each dtype in three shapes, and each state of a condition', () => {
    // Six positions of a step, alone and with each of four positions inside an expression.
    expect(grid.contexts).toHaveLength(30);
    // Nothing; sixty-seven classes of dtypes, each of one value, of many and of a count; four states of a condition.
    expect(grid.afters).toHaveLength(1 + 67 * 3 + 4);
    expect(found.cells).toBe(30 * 206);
    // The classes are the 26 the tables hold lines for, and the lists and arrays of each of 22 scalar
    // dtypes besides, which the view reads by the names the engine writes them with.
    const classes = new Set(
      grid.afters.flatMap((after) => (after.kind === 'value' ? [after.dtype] : [])),
    );
    expect(classes.size).toBe(67);
    for (const name of [
      'List(Int64)',
      'List(Float64)',
      'List(Datetime(us, UTC))',
      'Array(Int64, 2)',
      'Array(Decimal(10, 2), 2)',
    ]) {
      expect(classes).toContain(name);
    }
  });

  it('hold every entry that stands in an expression, and no step', () => {
    expect(grid.entries).toHaveLength(279);
    expect(grid.entries.filter((id) => id.startsWith('LazyFrame.'))).toEqual([]);
    for (const id of grid.entries) {
      expect(catalogue.entries.has(id), id).toBe(true);
    }
  });

  it('are the same where the host wrote the offers at three of them', () => {
    // The offers the engine gave at a place, which `offers_at` writes, allow what the grid does.
    for (const fixture of [offersStartFixture, offersValueFixture, offersConditionFixture]) {
      const offered = decodeOffers(fixture);
      const at = grid.contexts.findIndex(
        (context) =>
          context.outer === offered.place.context.outer &&
          context.nested === offered.place.context.nested,
      );
      const kind = grid.afters.findIndex(
        (after) => JSON.stringify(after) === JSON.stringify(offered.place.after),
      );
      const allowed = offered.entries
        .filter((entry) => entry.verdict.kind === 'allowed')
        .map((entry) => entry.entry);
      expect(allowed.length).toBeGreaterThan(0);
      expect(sorted(allowedAt(grid, at, kind))).toEqual(sorted(allowed));
    }
  });
});

/** Whether the first start of `kind` leads to a place of a value of the class `dtype` and shape `shape`. */
function leadsTo(kind: string, dtype: string, shape: 'one' | 'many' | 'counted'): boolean {
  return (
    STARTS.find((start) => start.kind === kind)?.leads({ kind: 'value', dtype, shape }) ?? false
  );
}

describe('the starts that are run', () => {
  it('are a field, a connected value, a connected list, a typed value of each kind a document holds, and each function that starts a chain', () => {
    expect(STARTS.map((start) => start.kind)).toEqual([
      'field',
      'connectedValue',
      'connectedList',
      'typedInteger',
      'typedFloat',
      'typedText',
      'typedFlag',
      ...FUNCTIONS.map(() => 'function'),
      'function',
    ]);
    // Thirteen functions start a chain besides `col` and `lit`; `when` leads into a condition.
    expect(FUNCTIONS).toHaveLength(13);
  });

  it('are read from the node the host reports, where there is one to read', () => {
    expect(STARTS.map((start) => start.name).slice(0, 3)).toEqual([
      'document_id',
      'as_of',
      'holidays',
    ]);
  });

  it('lead to the classes of dtype the host’s readings give typed text', () => {
    // The host reads a typed whole number as Int32, a number with a fraction as Float64, and text as
    // String, or as Date where it reads as a date.
    expect(leadsTo('typedInteger', 'Int32', 'one')).toBe(true);
    expect(leadsTo('typedInteger', 'Int64', 'one')).toBe(false);
    expect(leadsTo('typedFloat', 'Float64', 'one')).toBe(true);
    expect(leadsTo('typedText', 'String', 'one')).toBe(true);
    expect(leadsTo('typedText', 'Date', 'one')).toBe(true);
    expect(leadsTo('typedText', 'Int32', 'one')).toBe(false);
  });

  it('lead to the Boolean places of one value for a flag a document holds typed in, as the census gives it', () => {
    expect(leadsTo('typedFlag', 'Boolean', 'one')).toBe(true);
    expect(leadsTo('typedFlag', 'String', 'one')).toBe(false);
    expect(leadsTo('typedFlag', 'Boolean', 'many')).toBe(false);
  });

  it('lead to the places of one value for a connected value and a typed value, of many for a field, and of a count for a list', () => {
    expect(leadsTo('connectedValue', 'Int32', 'one')).toBe(true);
    expect(leadsTo('connectedValue', 'Int32', 'many')).toBe(false);
    expect(leadsTo('field', 'Int32', 'many')).toBe(true);
    expect(leadsTo('field', 'Int32', 'one')).toBe(false);
    // A list is the values of another table: a count of its own, and not the rows of this one.
    expect(leadsTo('connectedList', 'Int32', 'counted')).toBe(true);
    expect(leadsTo('connectedList', 'Int32', 'many')).toBe(false);
    expect(leadsTo('connectedList', 'Int32', 'one')).toBe(false);
  });
});

describe('the starts that are typed in', () => {
  it('are drawn by a word that lists the typed values, so that a value can be retyped where it stands', () => {
    // A typed value with methods after it has no other word to be retyped by: a start is changed by
    // the word it is drawn with.
    const typed = STARTS.filter(
      (start) => start.kind.startsWith('typed') && start.kind !== 'typedFlag',
    );
    expect(typed).toHaveLength(3);
    for (const start of typed) {
      const body = chainBodyOf(start.expression);
      expect(body.body, start.kind).toBe('typed');
      expect(listsOf(startWordOf(body)), start.kind).toContain('values');
    }
  });
});

describe('the grid, run through the functions the components call', () => {
  it('runs every cell of the grid', () => {
    expect(found.cells).toBe(30 * 206);
  });

  it('finds the gaps that are declared, and no gap that is not declared', () => {
    expect(compare(found, GAPS).undeclared).toEqual([]);
  });

  it('finds every gap that is declared, so that none is declared that the editor has drawn', () => {
    expect(compare(found, GAPS).stale).toEqual([]);
  });

  it('finds no start after which a method cannot be added, for a connected input and a typed value are drawn so that one can', () => {
    expect(
      [...found.gaps.values()].filter((gap) => gap.cover.kind === 'start').map((gap) => gap.cover),
    ).toEqual([]);
  });

  it('finds the entries the engine allows that no list offers, and no others', () => {
    // A further case of a condition follows `otherwise`, which the editor draws as one case; and
    // the editor starts a chain only from `when` and `element` besides a field and a typed value.
    expect(
      sorted(
        [...found.gaps.values()].flatMap((gap) =>
          gap.cover.kind === 'entry' ? [gap.cover.entry] : [],
        ),
      ),
    ).toEqual(
      sorted([
        'ChainedThen.otherwise',
        'ChainedThen.when',
        'ChainedWhen.then',
        'Then.when',
        'polars.date',
        'polars.date_range',
        'polars.datetime',
        'polars.datetime_range',
        'polars.duration',
        'polars.from_epoch',
        'polars.int_range',
        'polars.linear_space',
        'polars.repeat',
        'polars.struct',
        'polars.time',
        'polars.time_range',
      ]),
    );
  });

  it('finds a further case of a condition in the 24 contexts where the engine allows one', () => {
    // The engine allows the links of a further case in every context but the keys of a window in a
    // join key or a pivot's aggregate, 24 of the 30; each shows in one place of each.
    for (const id of [
      'ChainedThen.otherwise',
      'ChainedThen.when',
      'ChainedWhen.then',
      'Then.when',
    ]) {
      expect(found.gaps.get(`entry ${id}`)?.places, id).toBe(24);
    }
  });

  it('finds the parameters of a field, a struct, a typed value and a condition that the editor does not set', () => {
    expect(
      sorted(
        [...found.gaps.values()].flatMap((gap) =>
          gap.cover.kind === 'parameter' ? [`${gap.cover.entry} ${gap.cover.parameter}`] : [],
        ),
      ),
    ).toEqual([
      'Expr.struct.drop names',
      'Expr.struct.field name',
      'polars.col more_names',
      'polars.lit allow_object',
      'polars.lit dtype',
      'polars.when constraints',
    ]);
  });

  it('finds no kind of parameter without a drawing and no place that no word stands at', () => {
    expect([...found.gaps.values()].filter((gap) => gap.cover.kind === 'parameterKind')).toEqual(
      [],
    );
    expect([...found.gaps.values()].filter((gap) => gap.cover.kind === 'place')).toEqual([]);
  });
});

describe('a gap that is found and not declared, or declared and not found', () => {
  it('fails the check where the editor has no way to add after a start and the gap is not declared', () => {
    // A start that holds nothing picked is drawn as a word that nothing can follow.
    const unpicked: Start = {
      kind: 'connectedValue',
      name: 'nothing',
      expression: chain(HOLE_START),
      leads: (after) => after.kind === 'value' && after.shape === 'one',
    };
    const run = findGaps(grid, catalogue, [...STARTS, unpicked], grid.contexts);

    expect(compare(run, GAPS)).toEqual({ undeclared: ['start connectedValue'], stale: [] });
  });

  it('fails the check where a gap is declared that the editor has drawn, such as a method after a field', () => {
    const extra: Gap = { cover: { kind: 'start', start: 'field' }, reason: 'wrongly declared' };

    expect(compare(found, [...GAPS, extra])).toEqual({ undeclared: [], stale: ['start field'] });
  });

  it('fails the check where a gap is declared for an entry the editor offers, such as a method', () => {
    const extra: Gap = { cover: { kind: 'entry', entry: 'Expr.abs' }, reason: 'wrongly declared' };

    expect(compare(found, [...GAPS, extra])).toEqual({ undeclared: [], stale: ['entry Expr.abs'] });
  });

  it('fails the check where a gap is declared for a parameter the editor sets, such as the field of `col`', () => {
    const extra: Gap = {
      cover: { kind: 'parameter', entry: 'polars.col', parameter: 'name' },
      reason: 'wrongly declared',
    };

    expect(compare(found, [...GAPS, extra])).toEqual({
      undeclared: [],
      stale: ['parameter polars.col name'],
    });
  });

  it('fails the check where a parameter is not set and no gap is declared for it', () => {
    const without = GAPS.filter(
      (gap) =>
        !(
          gap.cover.kind === 'parameter' &&
          gap.cover.entry === 'polars.lit' &&
          gap.cover.parameter === 'dtype'
        ),
    );

    expect(compare(found, without)).toEqual({
      undeclared: ['parameter polars.lit dtype'],
      stale: [],
    });
  });

  it('is found at a place the grid allows but no word draws, such as a position nothing draws', () => {
    // A catalogue whose only step parameter at the filter position is not drawn has no word there.
    const sparse: Catalogue = {
      ...catalogue,
      entries: new Map([...catalogue.entries].filter(([id]) => id !== 'LazyFrame.filter')),
    };
    const run = findGaps(grid, sparse, STARTS, grid.contexts);

    expect([...run.gaps.values()].filter((gap) => gap.cover.kind === 'place')).toHaveLength(5);
    expect(run.gaps.has('place filter alone')).toBe(true);
    expect(run.gaps.has('place filter over.order_by')).toBe(true);
  });

  it('is found at an entry that no word lists, such as a method the catalogue gives no family', () => {
    // A method whose family the catalogue lacks has no row in any list, though the engine allows it.
    const orphan = { ...entryOf('Expr.abs'), family: 'no such family' };
    const orphaned: Catalogue = {
      ...catalogue,
      entries: new Map([...catalogue.entries, ['Expr.abs', orphan]]),
    };
    const run = findGaps(grid, orphaned, STARTS, grid.contexts);

    expect(run.gaps.has('entry Expr.abs')).toBe(true);
    expect(run.gaps.get('entry Expr.abs')?.places).toBeGreaterThan(100);
  });

  it('is found at a parameter of a kind with no drawing, such as pairs inside pairs', () => {
    const base = entryOf('Expr.filter');
    const constraints = base.parameters.find((parameter) => parameter.name === 'constraints');
    if (constraints === undefined) throw new Error('no constraints');
    const nested: EntryView = {
      ...base,
      parameters: [
        {
          ...constraints,
          kind: {
            kind: 'pairs',
            key: { kind: 'column' },
            value: { kind: 'pairs', key: { kind: 'column' }, value: { kind: 'flag' } },
          },
        },
      ],
    };
    const odd: Catalogue = {
      ...catalogue,
      entries: new Map([...catalogue.entries, ['Expr.filter', nested]]),
    };
    const run = findGaps(grid, odd, STARTS, grid.contexts);

    expect(run.gaps.has('parameterKind one of pairs')).toBe(true);
  });
});

describe('what the engine tells of an entry beside its place', () => {
  it('names a parameter of the entry that gets a control, for every argument and slot it starts with', () => {
    // The default argument, the parameter left out, and the slots to fill, which the engine says a
    // method starts with, are each a parameter of the method that the editor draws.
    const offered = decodeOffers(offersValueFixture);
    const named = new Set<string>();
    for (const entry of offered.entries) {
      if (entry.verdict.kind !== 'allowed') continue;
      const names = [
        ...(entry.argument === null ? [] : [entry.argument.parameter]),
        ...(entry.leftOut === null ? [] : [entry.leftOut]),
        ...entry.slots,
      ];
      const parameters = entryOf(entry.entry).parameters;
      for (const name of names) {
        const parameter = parameters.find((candidate) => candidate.name === name);
        expect(parameter, `${entry.entry} ${name}`).toBeDefined();
        if (parameter === undefined) continue;
        const setting = settingOf(entryOf(entry.entry), parameter);
        expect(['argument', 'option'], `${entry.entry} ${name}`).toContain(setting.by);
        if (setting.by === 'argument' || setting.by === 'option') {
          expect(isDrawn(setting.control), `${entry.entry} ${name}`).toBe(true);
        }
        named.add(`${entry.entry} ${name}`);
      }
    }
    // `bottom_k` starts with a 5, `cast` with its dtype to be given, and `dt.replace` leaves `year` at its default.
    expect(named).toContain('Expr.bottom_k k');
    expect(named).toContain('Expr.cast dtype');
    expect(named).toContain('Expr.dt.replace year');
    expect(named).toContain('Expr.dt.strftime format');
  });

  it('names an option of a join that is a flag or a choice, each of which is a control', () => {
    const options = decodeOptions(optionsFixture);
    const join = stepOptionParameters(catalogue, 'LazyFrame.join');
    expect(options.options.map((option) => option.parameter)).toEqual(
      expect.arrayContaining(['how', 'validate', 'nulls_equal', 'coalesce']),
    );
    for (const option of options.options) {
      const parameter = join.find((candidate) => candidate.name === option.parameter);
      expect(parameter, option.parameter).toBeDefined();
      expect(['flag', 'choice'], option.parameter).toContain(parameter?.kind.kind);
    }
  });
});
