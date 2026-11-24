import { describe, expect, it } from 'vitest';

import type { DocumentStateView, EditAnswerView, EditRequest } from '@dageist/host';
import { decodeDocumentState, HostError } from '@dageist/host';

import documentFixture from '../../host/fixtures/node-document.json';
import type { SessionListener, SessionPort } from './editSession';
import { createSession } from './editSession';

const base = decodeDocumentState(documentFixture);

/** The fixture's document, said to be at `revision`. */
function at(revision: number): DocumentStateView {
  return { ...base, revision };
}

/** An answer that says the edit was made and the document is now at `revision`. */
function edited(revision: number): EditAnswerView {
  return { status: 'edited', ...at(revision) };
}

const REMOVE: EditRequest = {
  kind: 'removeStep',
  step: { stepIndex: 0, stepId: 'sort', path: [] },
};
const MOVE: EditRequest = {
  kind: 'moveStep',
  step: { stepIndex: 1, stepId: 'step_index', path: [] },
  to: 0,
};

/** A promise settled from outside, so that a test decides in what order answers arrive. */
interface Deferred<T> {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (error: Error) => void;
}

function deferred<T>(): Deferred<T> {
  const settle: Array<(value: T) => void> = [];
  const fail: Array<(error: Error) => void> = [];
  const promise = new Promise<T>((done, failed) => {
    settle.push(done);
    fail.push(failed);
  });
  return {
    promise,
    resolve: (value) => {
      settle[0]?.(value);
    },
    reject: (error) => {
      fail[0]?.(error);
    },
  };
}

/** What a session told its window, and what it asked the host. */
interface Seen {
  readonly held: number[];
  readonly notices: string[];
  readonly failures: Array<string | null>;
  readonly broadcast: number[];
  readonly asked: Array<{ revision: number; kind: string }>;
  loads: number;
}

/** A session over a host that answers `answers` to each edit in turn and `loaded` to each load. */
function sessionOver(
  answers: ReadonlyArray<EditAnswerView | Error | Promise<EditAnswerView>>,
  loaded: ReadonlyArray<DocumentStateView | Promise<DocumentStateView>> = [at(1)],
) {
  const record: Seen = {
    held: [],
    notices: [],
    failures: [],
    broadcast: [],
    asked: [],
    loads: 0,
  };
  let nextAnswer = 0;
  const port: SessionPort = {
    load: async () => {
      const answer = loaded[Math.min(record.loads, loaded.length - 1)];
      record.loads += 1;
      if (answer === undefined) throw new Error('no load scripted');
      return answer;
    },
    edit: async (revision, edit) => {
      record.asked.push({ revision, kind: edit.kind });
      const answer = answers[nextAnswer];
      nextAnswer += 1;
      if (answer === undefined) throw new Error('no answer scripted');
      if (answer instanceof Error) throw answer;
      return answer;
    },
  };
  const listener: SessionListener = {
    held: (state) => {
      record.held.push(state.revision);
    },
    failure: (reason) => {
      record.failures.push(reason);
    },
    notice: (words) => {
      record.notices.push(words);
    },
    edited: (revision) => {
      record.broadcast.push(revision);
    },
  };
  return { session: createSession(port, listener), record };
}

describe('an action made on the document a window drew', () => {
  it('is made against the revision it was drawn at, not the newest one the window holds', async () => {
    const { session, record } = sessionOver([edited(3)], [at(2)]);
    await session.load();

    const sent = await session.act(1, [REMOVE]);

    // The window holds revision 2 but the person acted on what was drawn at 1.
    expect(record.asked).toEqual([{ revision: 1, kind: 'removeStep' }]);
    expect(sent.status).toBe('edited');
  });

  it('is sent as stale when the host has moved on, and the document is asked for again', async () => {
    const stale = new HostError(
      'edit_document',
      'stale',
      'the document is at revision 2 now, not 1',
    );
    const { session, record } = sessionOver([stale], [at(2)]);

    const sent = await session.act(1, [REMOVE]);

    expect(sent).toEqual({ status: 'stale' });
    expect(record.loads).toBe(1);
    expect(record.held).toEqual([2]);
    expect(record.notices).toEqual([
      'The steps changed since you did this, so it was not done. Try again.',
    ]);
  });

  it('cannot be made before the document has been had', async () => {
    const { session, record } = sessionOver([]);

    const sent = await session.act(null, [REMOVE]);

    expect(sent).toEqual({ status: 'failed', reason: 'The steps are not loaded yet.' });
    expect(record.asked).toEqual([]);
  });

  it('are made one after another, each started when the one before has been answered', async () => {
    const first = deferred<EditAnswerView>();
    const { session, record } = sessionOver([first.promise, edited(3)]);

    const one = session.act(1, [REMOVE]);
    const two = session.act(2, [MOVE]);
    await Promise.resolve();
    // The second waits while the first is out.
    expect(record.asked).toEqual([{ revision: 1, kind: 'removeStep' }]);

    first.resolve(edited(2));
    await Promise.all([one, two]);

    expect(record.asked).toEqual([
      { revision: 1, kind: 'removeStep' },
      { revision: 2, kind: 'moveStep' },
    ]);
  });
});

describe('an action that is several edits', () => {
  it('makes each against the revision the one before gave', async () => {
    const { session, record } = sessionOver([edited(2), edited(3)]);

    const sent = await session.act(1, [REMOVE, MOVE]);

    expect(record.asked).toEqual([
      { revision: 1, kind: 'removeStep' },
      { revision: 2, kind: 'moveStep' },
    ]);
    expect(sent.status === 'edited' && sent.state.revision).toBe(3);
    expect(record.held).toEqual([2, 3]);
    expect(record.broadcast).toEqual([2, 3]);
  });

  it('stops at the first edit the engine refuses, and says why', async () => {
    const refused: EditAnswerView = {
      status: 'refused',
      revision: 1,
      place: { stepIndex: 0, stepId: 'sort', path: [] },
      problem: { kind: 'nothing' },
      message: 'nothing is there',
    };
    const { session, record } = sessionOver([refused, edited(2)]);

    const sent = await session.act(1, [REMOVE, MOVE]);

    expect(record.asked).toEqual([{ revision: 1, kind: 'removeStep' }]);
    expect(sent).toEqual({
      status: 'refused',
      problem: { kind: 'nothing' },
      message: 'nothing is there',
    });
    expect(record.notices).toEqual(['nothing is there']);
  });

  it('does not tell the person of a refusal that only asks for a value', async () => {
    const needs: EditAnswerView = {
      status: 'refused',
      revision: 1,
      place: { stepIndex: 0, stepId: 'sort', path: [] },
      problem: { kind: 'needsValue', parameter: 'descending' },
      message: 'descending needs a value',
    };
    const { session, record } = sessionOver([needs]);

    const sent = await session.act(1, [REMOVE]);

    expect(sent.status).toBe('refused');
    expect(record.notices).toEqual([]);
  });

  it('stops at the first edit that is stale, and does not go on against the newer document', async () => {
    const stale = new HostError(
      'edit_document',
      'stale',
      'the document is at revision 4 now, not 1',
    );
    const { session, record } = sessionOver([stale, edited(5)], [at(4)]);

    const sent = await session.act(1, [REMOVE, MOVE]);

    expect(sent).toEqual({ status: 'stale' });
    expect(record.asked).toEqual([{ revision: 1, kind: 'removeStep' }]);
    expect(record.held).toEqual([4]);
  });

  it('does not take a failure of the host for a stale revision', async () => {
    const failure = new HostError('edit_document', 'failed', 'could not run the document');
    const { session, record } = sessionOver([failure], [at(2)]);

    const sent = await session.act(1, [REMOVE]);

    expect(sent).toEqual({ status: 'failed', reason: failure.message });
    expect(record.notices).toEqual([failure.message]);
  });

  it('stops where the host cannot be asked, and says what went wrong', async () => {
    const { session, record } = sessionOver([new Error('no network'), edited(2)]);

    const sent = await session.act(1, [REMOVE, MOVE]);

    expect(sent).toEqual({ status: 'failed', reason: 'no network' });
    expect(record.asked).toHaveLength(1);
    expect(record.notices).toEqual(['no network']);
  });

  it('asks for the document again after the host could not be asked, since what was drawn may be old', async () => {
    const { session, record } = sessionOver([new Error('no network')], [at(4)]);

    await session.act(1, [REMOVE]);

    expect(record.loads).toBe(1);
    expect(record.held).toEqual([4]);
  });
});

describe('the document asked for', () => {
  it('is taken whatever revision it is at, as after the host has been restarted', async () => {
    const { session, record } = sessionOver([], [at(5), at(1)]);

    await session.load();
    await session.load();

    expect(record.held).toEqual([5, 1]);
  });

  it('is ignored when a later request was made before it came, whichever way they arrive', async () => {
    const older = deferred<DocumentStateView>();
    const newer = deferred<DocumentStateView>();
    const { session, record } = sessionOver([], [older.promise, newer.promise]);

    const first = session.load();
    const second = session.load();
    newer.resolve(at(7));
    await second;
    older.resolve(at(6));
    await first;

    expect(record.held).toEqual([7]);
  });

  it('is ignored when an edit was answered after it was asked for', async () => {
    const slow = deferred<DocumentStateView>();
    const { session, record } = sessionOver([edited(3)], [slow.promise]);

    const loading = session.load();
    await session.act(2, [REMOVE]);
    slow.resolve(at(2));
    await loading;

    expect(record.held).toEqual([3]);
  });

  it('says nothing of a failure that a later request overtook, however late it comes', async () => {
    const late = deferred<DocumentStateView>();
    const { session, record } = sessionOver([], [late.promise, at(7)]);

    const first = session.load();
    await session.load();
    late.reject(new Error('the bridge went away'));
    await first;

    expect(record.held).toEqual([7]);
    expect(record.failures).toEqual([null]);
  });

  it('says why it could not be had, and says no more once it can', async () => {
    const { session, record } = sessionOver([], [at(1)]);
    const failing = createSession(
      {
        load: async () => {
          throw new Error('the bridge is not running');
        },
        edit: async () => {
          throw new Error('unused');
        },
      },
      {
        held: () => undefined,
        failure: (reason) => {
          record.failures.push(reason);
        },
        notice: () => undefined,
        edited: () => undefined,
      },
    );

    await failing.load();
    await session.load();

    expect(record.failures).toEqual(['the bridge is not running', null]);
  });

  it('is asked for again when another window says it is at a lower revision, as after a restart', async () => {
    const { session, record } = sessionOver([], [at(5), at(1)]);
    await session.load();

    session.heard(1);
    await Promise.resolve();
    await Promise.resolve();

    expect(record.loads).toBe(2);
    expect(record.held).toEqual([5, 1]);
  });

  it('is asked for again when another window says it is at another revision, and not at the same one', async () => {
    const { session, record } = sessionOver([], [at(2), at(3)]);
    await session.load();

    session.heard(2);
    session.heard(3);
    await Promise.resolve();
    await Promise.resolve();

    expect(record.loads).toBe(2);
  });
});

describe('a closed session', () => {
  it('tells its window nothing more', async () => {
    const answer = deferred<EditAnswerView>();
    const { session, record } = sessionOver([answer.promise]);

    const sending = session.act(1, [REMOVE]);
    session.close();
    answer.resolve(edited(2));
    await sending;

    expect(record.held).toEqual([]);
    expect(record.broadcast).toEqual([2]);
  });
});
