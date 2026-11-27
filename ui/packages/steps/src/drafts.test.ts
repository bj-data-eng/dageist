import { describe, expect, it } from 'vitest';

import { decodeDocumentState } from '@dageist/host';

import documentFixture from '../../host/fixtures/node-document.json';
import type { Draft } from './drafts';
import { stillWanted } from './drafts';
import { itemSegment, linkSegment, parameterSegment, valueSegment } from './model/doc';

const { document } = decodeDocumentState(documentFixture);

/** A draft of an item of a list, waiting in the step `step` for the parameter at `path`. */
function item(step: string, path: Draft['place']['path']): Draft {
  return { kind: 'item', place: { step, path } };
}

describe('a draft that is half done', () => {
  it('is wanted while the step and the call that would hold its parameter are there', () => {
    expect(stillWanted(document, item('sort', [parameterSegment('by')]))).toBe(true);
    // The parameter of a method deep in a chain: the call is the link.
    const inChain = [
      parameterSegment('named_exprs'),
      valueSegment(0),
      linkSegment(1),
      parameterSegment('partition_by'),
    ];
    expect(stillWanted(document, item('step_index', inChain))).toBe(true);
  });

  it('is not wanted once its step is gone, or the call that would hold it is', () => {
    expect(stillWanted(document, item('gone', [parameterSegment('by')]))).toBe(false);
    const missingLink = [
      parameterSegment('named_exprs'),
      valueSegment(0),
      linkSegment(9),
      parameterSegment('partition_by'),
    ];
    expect(stillWanted(document, item('step_index', missingLink))).toBe(false);
    // A way that does not end at a parameter is not the place of a draft.
    expect(stillWanted(document, item('sort', [parameterSegment('by'), itemSegment(0)]))).toBe(
      false,
    );
  });

  it('is kept when something elsewhere in the document changes, as a step added before its step', () => {
    // The place of a draft is the step by its name, so a step in front of it does not move it.
    const moved = {
      steps: [
        { id: 'new', call: { entry: 'LazyFrame.head', arguments: [] }, links: [] },
        ...document.steps,
      ],
    };

    expect(stillWanted(moved, item('sort', [parameterSegment('by')]))).toBe(true);
  });

  it('is an option no longer once the document gives the parameter', () => {
    const option: Draft = {
      kind: 'option',
      place: { step: 'join', path: [parameterSegment('how')] },
    };

    expect(stillWanted(document, option)).toBe(false);
    const without = {
      steps: document.steps.map((step) =>
        step.id === 'join'
          ? {
              ...step,
              call: {
                ...step.call,
                arguments: step.call.arguments.filter((given) => given.parameter !== 'how'),
              },
            }
          : step,
      ),
    };
    expect(stillWanted(without, option)).toBe(true);
  });
});
