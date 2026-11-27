import { describe, expect, it } from 'vitest';

import { decodeCatalogue } from '@dageist/host';

import catalogueFixture from '../../../host/fixtures/catalogue.json';
import stepsFile from '../../../host/fixtures/corpus-steps.json?raw';
import { decodeCorpusPart } from '../../../host/test-support/corpus';
import { finishingEntries, groupedRoleOf, havingEntry, indexCatalogue, verbOf } from './catalogue';
import { callInStep, linkSegment, parameterSegment } from './doc';
import { groupLayoutOf } from './group';

const catalogue = indexCatalogue(decodeCatalogue(catalogueFixture));
const documents = decodeCorpusPart(JSON.parse(stepsFile)).documents;

/** The step that groups of the corpus document called `name`. */
function groupedStep(name: string) {
  const found = documents.find((held) => held.name === name);
  const step = found?.document.steps[0];
  if (step === undefined) throw new Error(`no document ${name}`);
  return step;
}

describe('what a call on a group by is', () => {
  it('is said by the catalogue, which names the one that keeps groups and the calls that finish', () => {
    expect(havingEntry(catalogue)?.id).toBe('LazyGroupBy.having');
    const finishing = finishingEntries(catalogue).map((entry) => entry.id);
    expect(finishing).toContain('LazyGroupBy.agg');
    expect(finishing).toContain('LazyGroupBy.len');
    expect(finishing).not.toContain('LazyGroupBy.having');
    expect(groupedRoleOf(catalogue, 'LazyGroupBy.agg')).toBe('finishes');
    expect(groupedRoleOf(catalogue, 'Expr.sum')).toBeNull();
  });

  it('is nothing for a step the editor has no verb for the grouping of, and `group_by` is a verb', () => {
    expect(verbOf('LazyFrame.group_by')).toBe('group_by');
  });
});

describe('the layout of a step that groups', () => {
  it('has the predicate that keeps groups and the call that finishes, each with its place among the links', () => {
    const step = groupedStep('LazyGroupBy.having as it is put into a group by');
    const layout = groupLayoutOf(catalogue, step);
    expect(layout.having?.call.entry).toBe('LazyGroupBy.having');
    expect(layout.having?.at).toBe(0);
    expect(layout.finishing?.at).toBe(1);
    expect(groupedRoleOf(catalogue, layout.finishing?.call.entry ?? '')).toBe('finishes');
  });

  it('has no predicate where the step holds none, and still the call that finishes', () => {
    const step = groupedStep('LazyFrame.group_by as it is added');
    const layout = groupLayoutOf(catalogue, step);
    expect(layout.having).toBeNull();
    expect(layout.finishing?.at).toBe(0);
  });

  it('is empty for a step with no links, which the engine does not hold of a group by', () => {
    const step = groupedStep('LazyFrame.group_by as it is added');
    const bare = groupLayoutOf(catalogue, { ...step, links: [] });
    expect(bare).toEqual({ having: null, finishing: null });
  });
});

describe('the call a way through a step reaches', () => {
  it('is a call after `group_by` where the way begins with a link, and one inside its own call otherwise', () => {
    const step = groupedStep('LazyGroupBy.having as it is put into a group by');
    expect(callInStep(step, [linkSegment(0)])?.entry).toBe('LazyGroupBy.having');
    expect(callInStep(step, [linkSegment(1)])?.entry).toBe(step.links[1]?.entry);
    expect(callInStep(step, [linkSegment(9)])).toBeNull();
    expect(callInStep(step, [])?.entry).toBe('LazyFrame.group_by');
    // An argument is no call.
    expect(callInStep(step, [linkSegment(0), parameterSegment('predicates')])).toBeNull();
  });
});
