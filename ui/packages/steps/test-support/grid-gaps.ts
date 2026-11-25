/**
 * @file Test support: runs the model's functions over every cell of the grid the host wrote and
 * returns the gaps it finds, which a test compares with the declared gaps. It is not part of the
 * package's front door.
 */

import type {
  AfterView,
  ContextView,
  EntryView,
  OfferPlaceView,
  OffersView,
  ParameterView,
} from '@dageist/host';
import { decodeCatalogue, decodeNode } from '@dageist/host';

import catalogueFixture from '../../host/fixtures/catalogue.json';
import gridFixture from '../../host/fixtures/offers-grid.json';
import nodeInputsFixture from '../../host/fixtures/node-inputs.json';
import { allowedAt, decodeGrid } from '../../host/test-support/grid';
import type { Grid } from '../../host/test-support/grid';
import { indexCatalogue } from '../src/model/catalogue';
import type { Catalogue } from '../src/model/catalogue';
import { isDrawn, settingOf } from '../src/model/controls';
import type { ExpressionView } from '../src/model/doc';
import type { Cover, Gap, StartKind } from '../src/model/limits';
import { coverKey } from '../src/model/limits';
import { methodFamilies, startOffers } from '../src/model/offers';
import { isContextDrawn } from '../src/model/place';
import { actionsOfValue, canPick, routeOf, valueDrawing } from '../src/model/words';
import { bare, calls, chain, col, input, lit } from './expressions';
import { sorted } from './sorted';

export const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
export const grid = decodeGrid(gridFixture);
const node = decodeNode(nodeInputsFixture);

/** What an expression starts from, and which places of the grid that start leads to. */
export interface Start {
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
export const FUNCTIONS = [...catalogue.entries.values()].filter(
  (entry) => entry.applies === 'function' && entry.id !== 'polars.col' && entry.id !== 'polars.lit',
);

/** Every kind of start the editor has, as a chain that holds nothing but the start. */
export const STARTS: readonly Start[] = [
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
export interface Found {
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
export function entryOf(id: string): EntryView {
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
export function findGaps(
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
          .flatMap((row) => (row.stoppedBy !== 'engine' ? [row.id] : [])),
      );
      const functions = new Set(
        startOffers(using, view).flatMap((row) => (row.stoppedBy !== 'engine' ? [row.id] : [])),
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

/**
 * Returns the gaps found that `declared` lacks, and the gaps declared that were not found, by cover
 * key. The grid finds every kind of gap but the states of a report, which another test finds.
 */
export function compare(
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
