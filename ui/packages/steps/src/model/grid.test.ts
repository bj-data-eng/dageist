import { describe, expect, it } from 'vitest';

import type {
  AfterView,
  ContextView,
  EntryView,
  OfferPlaceView,
  OffersView,
  ParameterView,
} from '@dageist/host';
import { decodeCatalogue, decodeNode, decodeOffers, decodeOptions } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import gridFixture from '../../../host/fixtures/offers-grid.json';
import nodeInputsFixture from '../../../host/fixtures/node-inputs.json';
import offersConditionFixture from '../../../host/fixtures/offers-condition.json';
import offersStartFixture from '../../../host/fixtures/offers-start.json';
import offersValueFixture from '../../../host/fixtures/offers-value.json';
import optionsFixture from '../../../host/fixtures/options.json';
import { allowedAt, decodeGrid } from '../../../host/test-support/grid';
import type { Grid } from '../../../host/test-support/grid';
import { bare, calls, chain, col, input, lit } from '../../test-support/expressions';
import { sorted } from '../../test-support/sorted';
import { indexCatalogue, stepOptionParameters } from './catalogue';
import type { Catalogue } from './catalogue';
import { isDrawn, settingOf } from './controls';
import type { ExpressionView } from './doc';
import type { Cover, Gap, StartKind } from './limits';
import { GAPS, coverKey } from './limits';
import { methodFamilies, startOffers } from './offers';
import { isContextDrawn } from './place';
import { actionsOfValue, canPick, routeOf, valueDrawing } from './words';

// For every place the engine was asked about, every start that leads to it and every entry the
// engine allows there, this runs the model's functions, which the components call, and asks two
// things: whether the entry can be picked or added at that place, and whether each of its
// parameters gets a control. What it finds is compared with the declared gaps. The expectations
// are written from what the editor draws and what the grid means, not from what it prints.
const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const grid = decodeGrid(gridFixture);
const node = decodeNode(nodeInputsFixture);

/** What an expression starts from, and which places of the grid that start leads to. */
interface Start {
  readonly kind: StartKind;
  readonly name: string;
  readonly expression: ExpressionView;
  /** Whether the place right after the start is `after`: a value of this class and shape, or a condition. */
  readonly leads: (after: AfterView) => boolean;
}

/** A value place of the class `dtype` and shape `shape`, or any class where `dtype` is `null`. */
function valueAfter(dtype: string | null, shape: 'one' | 'many'): (after: AfterView) => boolean {
  return (after) =>
    after.kind === 'value' && after.shape === shape && (dtype === null || after.dtype === dtype);
}

/** The first field of the table the steps start from, the connected single value and the connected list. */
const FIELD = node.source.columns[0];
const VALUE = node.inputs.find((connected) => connected.kind === 'value');
const LIST = node.inputs.find((connected) => connected.kind === 'list');
if (FIELD === undefined || VALUE === undefined || LIST === undefined) {
  throw new Error('the node fixture holds no field, no connected value or no connected list');
}

/** Every function that starts a chain but `col` and `lit`, which are the field and the typed value. */
const FUNCTIONS = [...catalogue.entries.values()].filter(
  (entry) => entry.applies === 'function' && entry.id !== 'polars.col' && entry.id !== 'polars.lit',
);

/** Every kind of start the editor has, as a chain that holds nothing but the start. */
const STARTS: readonly Start[] = [
  {
    kind: 'field',
    name: FIELD.name,
    expression: chain(calls(col(FIELD.name))),
    leads: valueAfter(null, 'many'),
  },
  {
    kind: 'connectedValue',
    name: VALUE.name,
    expression: chain(input(VALUE.name)),
    leads: valueAfter(null, 'one'),
  },
  {
    kind: 'connectedList',
    name: LIST.name,
    expression: chain(input(LIST.name)),
    leads: valueAfter(null, 'many'),
  },
  {
    kind: 'typedInteger',
    name: '5',
    expression: chain(calls(lit({ kind: 'integer', shown: '5' }))),
    leads: valueAfter('Int64', 'one'),
  },
  {
    kind: 'typedFloat',
    name: '1.5',
    expression: chain(calls(lit({ kind: 'float', shown: '1.5' }))),
    leads: valueAfter('Float64', 'one'),
  },
  {
    kind: 'typedText',
    name: 'text',
    expression: chain(calls(lit({ kind: 'string', value: 'text' }))),
    leads: valueAfter('String', 'one'),
  },
  // What a function gives is the engine's to say, so a function leads to every value place.
  ...FUNCTIONS.map((entry): Start => ({
    kind: 'function',
    name: entry.id,
    expression: chain(calls(bare(entry.id))),
    leads: (after) => after.kind === 'value',
  })),
  {
    kind: 'function',
    name: 'polars.when',
    expression: chain(calls(bare('polars.when'))),
    leads: (after) => after.kind === 'condition',
  },
];

/** What a run of the model over the grid found, and how large the grid was. */
interface Found {
  /** The gaps found, by cover key, each with how many places showed it. */
  readonly gaps: ReadonlyMap<string, { readonly cover: Cover; readonly places: number }>;
  readonly cells: number;
  /** The cells where the engine allows at least one entry. */
  readonly nonEmpty: number;
  /** Each start that leads to a cell, counted for every such cell. */
  readonly starts: number;
  /** Each entry the engine allows at a cell, counted for every start that leads to it. */
  readonly entries: number;
}

/** The engine's answer to `place` as far as the grid says: every entry of `allowed` is allowed. */
function offersAt(place: OfferPlaceView, allowed: readonly string[]): OffersView {
  return {
    place,
    entries: allowed.map((entry) => ({
      entry,
      verdict: {
        kind: 'allowed',
        result: null,
        shape: 'one',
        ending: { kind: 'allowed' },
      },
      argument: null,
      leftOut: null,
      slots: [],
    })),
  };
}

/** The entry the catalogue has under `id`; one it lacks is a gap of the grid itself. */
function entryOf(id: string): EntryView {
  const found = catalogue.entries.get(id);
  if (found === undefined) throw new Error(`the catalogue lacks ${id}`);
  return found;
}

/** Returns a description of a parameter's control that is not drawn, as a kind of parameter. */
function undrawnKind(parameter: ParameterView): string {
  return `${parameter.shape.shape} of ${parameter.kind.kind}`;
}

/**
 * Runs the model's functions for every cell of `grid`, every start that leads to it and every
 * entry allowed there, and returns the gaps it found.
 */
function findGaps(
  over: Grid,
  using: Catalogue,
  starts: readonly Start[],
  contexts: readonly ContextView[],
): Found {
  const gaps = new Map<string, { cover: Cover; places: number }>();
  const note = (cover: Cover): void => {
    const key = coverKey(cover);
    gaps.set(key, { cover, places: (gaps.get(key)?.places ?? 0) + 1 });
  };
  let cells = 0;
  let nonEmpty = 0;
  let startsRun = 0;
  let entriesRun = 0;

  // A place no word of the editor stands at.
  for (const context of contexts) {
    if (!isContextDrawn(using, context)) {
      note({ kind: 'place', outer: context.outer, nested: context.nested });
    }
  }

  for (const [at, context] of over.contexts.entries()) {
    const startsHere = allowedAt(over, at, 0);
    for (const [kind, after] of over.afters.entries()) {
      cells += 1;
      const allowed = allowedAt(over, at, kind);
      if (allowed.length === 0) continue;
      nonEmpty += 1;
      const place: OfferPlaceView = { context, after };
      // Which entries a list has a row for here, which the engine allows.
      const view = offersAt(place, allowed);
      const methods = new Set(
        methodFamilies(using, view)
          .flatMap((family) => family.offers)
          .flatMap((row) => (row.reason === null ? [row.id] : [])),
      );
      const functions = new Set(
        startOffers(using, view).flatMap((row) => (row.reason === null ? [row.id] : [])),
      );

      // Every start that leads here must have a way to add what the engine allows after it.
      const leading = after.kind === 'start' ? [] : starts.filter((start) => start.leads(after));
      for (const start of leading) {
        if (!actionsOfValue(valueDrawing(start.expression, true), false).add) {
          note({ kind: 'start', start: start.kind });
        }
      }
      startsRun += after.kind === 'start' ? 1 : leading.length;
      entriesRun += allowed.length * (after.kind === 'start' ? 1 : leading.length);

      // Every entry the engine allows must be one that can be picked, or made, here.
      for (const id of allowed) {
        const entry = using.entries.get(id);
        if (entry === undefined) {
          note({ kind: 'entry', entry: id });
          continue;
        }
        const route = routeOf(entry);
        let reached = canPick(entry, after.kind);
        if (reached && route.route === 'group' && route.group === 'methods') {
          reached = methods.has(id);
        }
        if (reached && route.route === 'group' && route.group === 'functions') {
          reached = functions.has(id);
        }
        // A link made with `when` is there only where `when` can start the expression.
        if (reached && route.route === 'with') reached = startsHere.includes('polars.when');
        if (!reached) note({ kind: 'entry', entry: id });
      }
    }
  }

  // Every parameter of every entry the editor can pick must get a control.
  for (const entry of using.entries.values()) {
    if (!over.entries.includes(entry.id) || routeOf(entry).route === 'none') continue;
    for (const parameter of entry.parameters) {
      const setting = settingOf(entry, parameter);
      if (setting.by === 'unset') {
        note({ kind: 'parameter', entry: entry.id, parameter: parameter.name });
      } else if (
        (setting.by === 'argument' || setting.by === 'option') &&
        !isDrawn(setting.control)
      ) {
        note({ kind: 'parameterKind', of: undrawnKind(parameter) });
      }
    }
  }
  return { gaps, cells, nonEmpty, starts: startsRun, entries: entriesRun };
}

/** The run over the grid the host wrote. */
const found = findGaps(grid, catalogue, STARTS, grid.contexts);

/**
 * Returns the gaps found that `declared` lacks, and the gaps declared that were not found, by cover
 * key. The grid finds every kind of gap but the states of a report, which another test finds.
 */
function compare(
  discovered: Found,
  declared: readonly Gap[],
): { readonly undeclared: string[]; readonly stale: string[] } {
  const declaredKeys = new Set(
    declared.filter((gap) => gap.cover.kind !== 'report').map((gap) => coverKey(gap.cover)),
  );
  return {
    undeclared: sorted([...discovered.gaps.keys()].filter((key) => !declaredKeys.has(key))),
    stale: sorted([...declaredKeys].filter((key) => !discovered.gaps.has(key))),
  };
}

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
    expect(grid.entries).toHaveLength(268);
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

describe('the starts that lead to the places', () => {
  it('are a field, a connected value, a connected list, a typed value of each kind, and each function that starts a chain', () => {
    expect(STARTS.map((start) => start.kind)).toEqual([
      'field',
      'connectedValue',
      'connectedList',
      'typedInteger',
      'typedFloat',
      'typedText',
      ...FUNCTIONS.map(() => 'function'),
      'function',
    ]);
    // Twelve functions start a chain besides `col` and `lit`; `when` leads into a condition.
    expect(FUNCTIONS).toHaveLength(12);
    expect(STARTS).toHaveLength(6 + 12 + 1);
  });

  it('are read from the node the host reports, where there is one to read', () => {
    expect(STARTS.map((start) => start.name).slice(0, 3)).toEqual(['dcn', 'as_of', 'holidays']);
  });
});

describe('the grid, run through the functions the components call', () => {
  it('covers every place, every start that leads to it and every entry allowed there', () => {
    // 6180 places, of which the engine allows something at 4944; 358362 combinations of a place and
    // an entry, which a start leads to in turn.
    expect(found.cells).toBe(6180);
    expect(found.nonEmpty).toBe(4944);
    expect(found.starts).toBeGreaterThan(found.nonEmpty);
    expect(found.entries).toBeGreaterThan(358_362);
  });

  it('finds the gaps that are declared, and no gap that is not declared', () => {
    expect(compare(found, GAPS).undeclared).toEqual([]);
  });

  it('finds every gap that is declared, so that none is declared that the editor has drawn', () => {
    expect(compare(found, GAPS).stale).toEqual([]);
  });

  it('finds nothing after a connected input or a typed value, though the engine allows methods there', () => {
    // A connected single value or list and a typed whole number, number or text is drawn as one
    // word, which has no way to add a method after it (#171). A field and a function are drawn as chains.
    expect(
      [...found.gaps.values()].filter((gap) => gap.cover.kind === 'start').map((gap) => gap.cover),
    ).toEqual([
      { kind: 'start', start: 'connectedValue' },
      { kind: 'start', start: 'connectedList' },
      { kind: 'start', start: 'typedInteger' },
      { kind: 'start', start: 'typedFloat' },
      { kind: 'start', start: 'typedText' },
    ]);
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
        'polars.time',
        'polars.time_range',
      ]),
    );
  });

  it('finds the parameters of a field, a typed value and a condition that the editor does not set', () => {
    expect(
      sorted(
        [...found.gaps.values()].flatMap((gap) =>
          gap.cover.kind === 'parameter' ? [`${gap.cover.entry} ${gap.cover.parameter}`] : [],
        ),
      ),
    ).toEqual([
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
  it('fails the check where the editor has no way to add after a connected input and the gap is not declared', () => {
    const without = GAPS.filter(
      (gap) => !(gap.cover.kind === 'start' && gap.cover.start === 'connectedValue'),
    );

    expect(compare(found, without)).toEqual({
      undeclared: ['start connectedValue'],
      stale: [],
    });
  });

  it('fails the check where a gap is declared that the editor has drawn, such as a method after a field', () => {
    const extra: Gap = { cover: { kind: 'start', start: 'field' }, reason: 'wrongly declared' };

    expect(compare(found, [...GAPS, extra])).toEqual({ undeclared: [], stale: ['start field'] });
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
