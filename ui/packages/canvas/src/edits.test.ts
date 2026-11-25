import { describe, expect, it } from 'vitest';

import type { FrameView, GraphEdit, GraphEditAnswerView, GraphView } from '@dageist/host';
import { decodeGraph } from '@dageist/host';

import graphFixture from '../../host/fixtures/graph.json';
import { GraphEdits } from './edits';
import type { EditPort } from './edits';

// What the host writes of the graph the application opens on.
const start = decodeGraph(graphFixture);

// The ids the host gives two of the starting graph's nodes.
const CLAIMS = '00000000-0000-7000-8000-000000000001';
const PARAMETERS = '00000000-0000-7000-8000-000000000005';

/** A host that holds revisions as the real one does: an edit at another revision is refused. */
function fakeHost(): {
  readonly port: EditPort;
  readonly sent: string[];
} {
  let revision = start.revision;
  let layout = start.layoutRevision;
  const sent: string[] = [];
  const port: EditPort = {
    editGraph: async (asked: number, edit: GraphEdit): Promise<GraphEditAnswerView> => {
      await Promise.resolve();
      sent.push(`edit ${edit.edit} at ${String(asked)}`);
      if (asked !== revision) throw new Error(`stale: ${String(asked)} is not ${String(revision)}`);
      if (edit.edit === 'rename' && edit.name === '') {
        return { kind: 'refused', revision, message: 'the name is empty' };
      }
      revision += 1;
      const added = edit.edit === 'add' ? 'transform' : null;
      return { kind: 'edited', graph: { ...start, revision, layoutRevision: layout }, added };
    },
    placeNode: async (asked: number, node: string, _frame: FrameView) => {
      await Promise.resolve();
      sent.push(`place ${node} at ${String(asked)}`);
      if (asked !== layout) throw new Error(`stale: ${String(asked)} is not ${String(layout)}`);
      layout += 1;
      return { layoutRevision: layout };
    },
  };
  return { port, sent };
}

/** The queue on a fake host, with what it reported. */
function queued(): {
  readonly edits: GraphEdits;
  readonly sent: string[];
  readonly views: GraphView[];
  readonly refusals: string[];
  readonly failures: unknown[];
} {
  const { port, sent } = fakeHost();
  const views: GraphView[] = [];
  const refusals: string[] = [];
  const failures: unknown[] = [];
  const edits = new GraphEdits(port, start, {
    view: (view) => {
      views.push(view);
    },
    refused: (message) => {
      refusals.push(message);
    },
    failed: (error) => {
      failures.push(error);
    },
  });
  return { edits, sent, views, refusals, failures };
}

const FRAME: FrameView = { x: 10, y: 20, width: null, height: null };

describe('the queue of edits', () => {
  it('places several boxes asked for at once, each at the revision the one before gave', async () => {
    const { edits, sent, failures, views } = queued();
    await Promise.all([edits.place(CLAIMS, FRAME), edits.place(PARAMETERS, FRAME)]);
    expect(failures).toEqual([]);
    const layout = start.layoutRevision;
    expect(sent).toEqual([
      `place ${CLAIMS} at ${String(layout)}`,
      `place ${PARAMETERS} at ${String(layout + 1)}`,
    ]);
    expect(views.at(-1)?.layoutRevision).toBe(layout + 2);
    expect(views.at(-1)?.nodes.find((node) => node.id === PARAMETERS)?.frame).toEqual(FRAME);
  });

  it('removes several boxes asked for at once, each at the revision the one before gave', async () => {
    const { edits, sent, failures } = queued();
    await Promise.all([
      edits.edit({ edit: 'remove', node: CLAIMS }),
      edits.edit({ edit: 'remove', node: PARAMETERS }),
    ]);
    expect(failures).toEqual([]);
    expect(sent).toEqual([
      `edit remove at ${String(start.revision)}`,
      `edit remove at ${String(start.revision + 1)}`,
    ]);
  });

  it('places a box it added where it was asked to, after the edit that added it', async () => {
    const { edits, sent } = queued();
    await edits.edit({ edit: 'add', kind: 'transform', follows: CLAIMS, name: null }, () => ({
      x: 400,
      y: 0,
    }));
    expect(sent).toEqual([
      `edit add at ${String(start.revision)}`,
      `place transform at ${String(start.layoutRevision)}`,
    ]);
  });

  it('says why an edit was refused and keeps the revision for the next', async () => {
    const { edits, sent, refusals } = queued();
    await edits.edit({ edit: 'rename', node: CLAIMS, name: '' });
    await edits.edit({ edit: 'remove', node: CLAIMS });
    expect(refusals).toEqual(['the name is empty']);
    expect(sent.at(-1)).toBe(`edit remove at ${String(start.revision)}`);
  });

  it('reports a failure and goes on with the edits after it', async () => {
    const { edits, failures, sent } = queued();
    edits.seen({ ...start, revision: start.revision - 1 });
    await edits.edit({ edit: 'remove', node: CLAIMS });
    edits.seen(start);
    await edits.edit({ edit: 'remove', node: CLAIMS });
    expect(failures).toHaveLength(1);
    expect(sent.at(-1)).toBe(`edit remove at ${String(start.revision)}`);
  });
});
