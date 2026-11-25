import { describe, expect, it } from 'vitest';

import censusFixture from '../../../host/fixtures/census.json';
import methodsFile from '../../../host/fixtures/corpus-methods.json?raw';
import parametersFile from '../../../host/fixtures/corpus-parameters.json?raw';
import refusedFile from '../../../host/fixtures/corpus-refused.json?raw';
import startsFile from '../../../host/fixtures/corpus-starts.json?raw';
import stepsFile from '../../../host/fixtures/corpus-steps.json?raw';
import type { ArgumentView, CallView, EntryView } from '@dageist/host';

import { decodeCensus } from '../../../host/test-support/census';
import { decodeCorpusPart } from '../../../host/test-support/corpus';
import type { CorpusDocument, CorpusPart } from '../../../host/test-support/corpus';
import { compareCorpus, heldBy, missingFrom, rebuildCorpus } from '../../test-support/corpus-gaps';
import type { Wanted } from '../../test-support/corpus-gaps';
import { catalogue, entryOf, grid } from '../../test-support/grid-gaps';
import { sorted } from '../../test-support/sorted';
import type { Catalogue } from './catalogue';
import type { Gap } from './limits';
import { GAPS } from './limits';

/** Reads a file of the corpus as the host wrote it. */
function part(text: string): CorpusPart {
  const value: unknown = JSON.parse(text);
  return decodeCorpusPart(value);
}

const PARTS = {
  starts: part(startsFile),
  methods: part(methodsFile),
  parameters: part(parametersFile),
  steps: part(stepsFile),
  refused: part(refusedFile),
};
const DOCUMENTS: readonly CorpusDocument[] = Object.values(PARTS).flatMap((held) => held.documents);
/** What every document is run with: each file says so, and says the same. */
const INPUTS = PARTS.starts.inputs;
const census = decodeCensus(censusFixture);

/** What the engine and the host list that the corpus is to hold, written from the grid, the catalogue and the census. */
const WANTED: Wanted = {
  expressionEntries: grid.entries,
  catalogue,
  contexts: grid.contexts,
  starts: census.starts.map((start) => {
    switch (start.kind) {
      case 'typed':
        return `typed ${start.literal}`;
      case 'function':
        return `function ${start.entry}`;
      case 'field':
      case 'connectedValue':
      case 'connectedList':
      case 'hole':
        return start.kind;
      default:
        return start satisfies never;
    }
  }),
  outcomes: census.outcomes,
  causes: census.causes,
};

// Every document the engine made by its own edits, with the report the host gives it, rebuilt from
// what the model offers: for each piece, the action that made it. The expectations are written from
// what the catalogue and the editor mean, not from what the run prints.
const found = rebuildCorpus(DOCUMENTS, INPUTS, catalogue);

describe('the corpus the host wrote', () => {
  it('is five files of documents, each with a name that says what it is for', () => {
    expect(Object.keys(PARTS)).toEqual(['starts', 'methods', 'parameters', 'steps', 'refused']);
    const names = DOCUMENTS.map((held) => held.name);
    expect(new Set(names).size).toBe(names.length);
    for (const held of Object.values(PARTS)) {
      expect(held.documents.length).toBeGreaterThan(0);
    }
    // Every document is run on the same node, which is connected to a value, a list and two tables.
    for (const held of Object.values(PARTS)) expect(held.inputs).toEqual(INPUTS);
    expect(INPUTS.map((input) => `${input.kind} ${input.name}`)).toEqual([
      'value limit',
      'list names',
      'table other',
      'table pivot_columns',
    ]);
  });

  it('holds every entry that stands in an expression, every step, every parameter the engine offers, every context, every kind of start, every outcome and every cause', () => {
    expect(missingFrom(heldBy(DOCUMENTS, INPUTS), WANTED)).toEqual([]);
  });

  it('holds documents the engine allows and documents it refuses', () => {
    const outcomes = new Set(
      DOCUMENTS.flatMap((held) => held.run.steps.map((step) => step.outcome.status)),
    );
    expect(outcomes).toEqual(new Set(['valid', 'refused', 'incomplete', 'notRun']));
    expect(PARTS.refused.documents.length).toBeGreaterThan(0);
  });
});

describe('the corpus, rebuilt through the functions the components call', () => {
  it('rebuilds every piece of every document, and finds nothing the model should offer an action for and does not', () => {
    expect(found.failures).toEqual([]);
    // A step, a start and a method are each a piece, and so is each argument and option.
    expect(found.pieces).toBeGreaterThan(heldBy(DOCUMENTS, INPUTS).calls);
  });

  it('finds the gaps that are declared, and no gap that is not declared', () => {
    expect(compareCorpus(found, GAPS).undeclared).toEqual([]);
  });

  it('finds every gap that is declared of a kind it finds, so that none is declared that the editor has drawn', () => {
    expect(compareCorpus(found, GAPS).stale).toEqual([]);
  });

  it('finds a step no list adds and a parameter of a step that is not offered, as the catalogue says', () => {
    // The step list holds the seven steps with heads; the other thirteen are drawn by their parameters.
    const steps = [...found.gaps.values()].flatMap((gap) =>
      gap.cover.kind === 'step' ? [gap.cover.entry] : [],
    );
    expect(sorted(steps)).toEqual([
      'LazyFrame.drop',
      'LazyFrame.drop_nans',
      'LazyFrame.drop_nulls',
      'LazyFrame.fill_nan',
      'LazyFrame.first',
      'LazyFrame.gather_every',
      'LazyFrame.head',
      'LazyFrame.last',
      'LazyFrame.limit',
      'LazyFrame.reverse',
      'LazyFrame.slice',
      'LazyFrame.tail',
      'LazyFrame.with_row_index',
    ]);
    const parameters = [...found.gaps.values()].flatMap((gap) =>
      gap.cover.kind === 'stepParameter' ? [`${gap.cover.entry} ${gap.cover.parameter}`] : [],
    );
    expect(sorted(parameters)).toEqual(['LazyFrame.join on', 'LazyFrame.sort more_by']);
  });

  it('finds no start that a method cannot follow, now that a connected value, a connected list and a typed value can have one added', () => {
    const starts = [...found.gaps.values()].flatMap((gap) =>
      gap.cover.kind === 'start' ? [gap.cover.start] : [],
    );
    expect(starts).toEqual([]);
  });

  it('places every expression of a step that was run, and finds the one the walk gives no context', () => {
    // The engine walks the value of a keyword constraint of a filter and names no position for
    // it, so no list can be asked what may stand there: a declared gap. An expression of a step
    // that was not run has no walk at all, and the corpus holds none.
    expect(found.unplaced).toEqual([]);
    expect([...found.gaps.keys()].filter((key) => key.startsWith('noContext'))).toEqual([
      'noContext LazyFrame.filter constraints',
    ]);
  });
});

/** Returns the documents that hold no call of `entry`. */
function without(entry: string): CorpusDocument[] {
  return DOCUMENTS.filter((held) => !heldBy([held], INPUTS).entries.has(entry));
}

/** Returns `call` with the argument for `parameter` of any call of `entry` inside it replaced by `value`. */
function replacedIn(
  call: CallView,
  entry: string,
  parameter: string,
  value: ArgumentView,
): CallView {
  const change = (argument: ArgumentView): ArgumentView => {
    switch (argument.kind) {
      case 'expression':
        return {
          ...argument,
          start:
            argument.start.kind === 'call'
              ? {
                  kind: 'call',
                  call: replacedIn(argument.start.call, entry, parameter, value),
                }
              : argument.start,
          links: argument.links.map((link) => replacedIn(link, entry, parameter, value)),
        };
      case 'many':
        return { ...argument, items: argument.items.map(change) };
      case 'pairs':
        return {
          ...argument,
          entries: argument.entries.map((pair) => ({
            key: change(pair.key),
            value: change(pair.value),
          })),
        };
      default:
        return argument;
    }
  };
  return {
    ...call,
    arguments: call.arguments.map((given) => ({
      parameter: given.parameter,
      value: call.entry === entry && given.parameter === parameter ? value : change(given.value),
    })),
  };
}

/** Returns the first document that holds a call of `entry` given `parameter`, with that argument replaced. */
function withArgument(entry: string, parameter: string, value: ArgumentView): CorpusDocument {
  const held = DOCUMENTS.find((candidate) =>
    heldBy([candidate], INPUTS).parameters.has(`${entry} ${parameter}`),
  );
  if (held === undefined) throw new Error(`no document gives ${entry} ${parameter}`);
  return {
    ...held,
    document: {
      steps: held.document.steps.map((step) => ({
        ...step,
        call: replacedIn(step.call, entry, parameter, value),
      })),
    },
  };
}

describe('a corpus that lacks something, or holds what the editor cannot draw', () => {
  it('fails the check of what it holds where it has no document that starts a chain from a function, and names the function', () => {
    const thinner = without('polars.date_range');

    expect(missingFrom(heldBy(thinner, INPUTS), WANTED)).toEqual(
      expect.arrayContaining(['entry polars.date_range', 'start function polars.date_range']),
    );
  });

  it('fails the check where it has no document that holds a step, and names the step', () => {
    expect(missingFrom(heldBy(without('LazyFrame.pivot'), INPUTS), WANTED)).toContain(
      'step LazyFrame.pivot',
    );
  });

  it('fails the check where no document gives a parameter the engine offers, and names it', () => {
    const thinner = DOCUMENTS.filter(
      (held) => !heldBy([held], INPUTS).parameters.has('LazyFrame.sort descending'),
    );

    expect(missingFrom(heldBy(thinner, INPUTS), WANTED)).toContain(
      'parameter LazyFrame.sort descending',
    );
  });

  it('fails the check where no document starts a chain as the census lists, such as with nothing picked', () => {
    const picked = DOCUMENTS.filter((held) => !heldBy([held], INPUTS).starts.has('hole'));

    expect(missingFrom(heldBy(picked, INPUTS), WANTED)).toContain('start hole');
  });

  it('finds a piece of a kind the editor cannot draw, and names the document that holds it', () => {
    // A parameter of pairs whose value is pairs again has no control. The corpus gives `Expr.filter`
    // its keyword constraints in one document.
    const base = entryOf('Expr.filter');
    const constraints = base.parameters.find((parameter) => parameter.name === 'constraints');
    if (constraints === undefined) throw new Error('no constraints');
    const nested: EntryView = {
      ...base,
      parameters: base.parameters.map((parameter) =>
        parameter.name === 'constraints'
          ? {
              ...constraints,
              kind: {
                kind: 'pairs',
                key: { kind: 'column' },
                value: { kind: 'pairs', key: { kind: 'column' }, value: { kind: 'flag' } },
              },
            }
          : parameter,
      ),
    };
    const odd: Catalogue = {
      ...catalogue,
      entries: new Map([...catalogue.entries, ['Expr.filter', nested]]),
    };
    const run = rebuildCorpus(DOCUMENTS, INPUTS, odd);

    expect(compareCorpus(run, GAPS).undeclared).toContain('parameterKind one of pairs');
    expect(run.gaps.get('parameterKind one of pairs')?.pieces[0]).toContain('Expr.filter');
  });

  it('finds an entry no list has a row for, and names the documents that hold it', () => {
    // A method whose family the catalogue lacks has no row in any list, though the engine allows it.
    const orphan = { ...entryOf('Expr.abs'), family: 'no such family' };
    const orphaned: Catalogue = {
      ...catalogue,
      entries: new Map([...catalogue.entries, ['Expr.abs', orphan]]),
    };
    const run = rebuildCorpus(DOCUMENTS, INPUTS, orphaned);

    expect(compareCorpus(run, GAPS).undeclared).toEqual(['entry Expr.abs']);
    const pieces = run.gaps.get('entry Expr.abs')?.pieces ?? [];
    expect(pieces.length).toBeGreaterThan(1);
    expect(pieces.every((piece) => piece.includes('link'))).toBe(true);
  });

  it('fails the check where a gap is declared that the corpus no longer shows, such as a step the editor adds', () => {
    const extra: Gap = {
      cover: { kind: 'step', entry: 'LazyFrame.select' },
      reason: 'wrongly declared as a step no list adds',
    };
    const parameter: Gap = {
      cover: { kind: 'stepParameter', entry: 'LazyFrame.sort', parameter: 'by' },
      reason: 'wrongly declared as a parameter a step does not offer',
    };

    expect(compareCorpus(found, [...GAPS, extra, parameter])).toEqual({
      undeclared: [],
      stale: ['step LazyFrame.select', 'stepParameter LazyFrame.sort by'],
    });
  });

  it('fails the check where an expression the walk gives no context is not declared', () => {
    const declared = GAPS.filter((gap) => gap.cover.kind !== 'noContext');

    expect(compareCorpus(found, declared)).toEqual({
      undeclared: ['noContext LazyFrame.filter constraints'],
      stale: [],
    });
  });

  it('fails the check where a piece the corpus shows is not declared, such as a step no list adds', () => {
    const declared = GAPS.filter(
      (gap) => !(gap.cover.kind === 'step' && gap.cover.entry === 'LazyFrame.head'),
    );

    expect(compareCorpus(found, declared)).toEqual({
      undeclared: ['step LazyFrame.head'],
      stale: [],
    });
  });

  it('does not ask the corpus to find a gap that another test finds, nor to leave one out', () => {
    // `typed flag` is found by the census test and `entry polars.date` by the grid test: the corpus
    // may show the second and is not asked for either.
    const keys = GAPS.map((gap) => gap.cover).filter((cover) => cover.kind === 'typed');
    expect(keys).toHaveLength(1);
    expect(compareCorpus(found, GAPS).stale).toEqual([]);
  });

  it('fails the rebuild where an argument is not of the kind its control draws, and names the piece', () => {
    // A flag a method is given, held as text where the control draws a flag.
    const odd = withArgument('Expr.arg_sort', 'descending', { kind: 'string', value: 'yes' });
    const run = rebuildCorpus([odd], INPUTS, catalogue);

    expect(run.failures).toHaveLength(1);
    expect(run.failures[0]).toContain('Expr.arg_sort');
    expect(run.failures[0]).toContain('descending');
    expect(run.failures[0]).toContain('a string, where flag is drawn');
  });

  it('fails the rebuild where a step that was accepted says nothing, and names the step', () => {
    const accepted = DOCUMENTS.find((held) =>
      held.run.steps.some((step) => step.outcome.status === 'valid'),
    );
    if (accepted === undefined) throw new Error('no document holds an accepted step');
    const silent: CorpusDocument = {
      ...accepted,
      run: {
        ...accepted.run,
        steps: accepted.run.steps.map((step) => ({ ...step, count: { kind: 'notCounted' } })),
      },
    };
    const run = rebuildCorpus([silent], INPUTS, catalogue);

    expect(run.failures[0]).toContain('the editor says nothing of the step');
    expect(run.failures[0]).toContain(accepted.name);
  });
});
