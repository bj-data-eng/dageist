import { describe, expect, it } from 'vitest';

import type { Outcome, Timers } from './latest';
import { Latest } from './latest';

/** A clock that only moves when the test says. */
class Clock implements Timers {
  private now = 0;
  private next = 1;
  private readonly waiting = new Map<number, { readonly at: number; readonly work: () => void }>();

  readonly start = (work: () => void, milliseconds: number): number => {
    const handle = this.next;
    this.next += 1;
    this.waiting.set(handle, { at: this.now + milliseconds, work });
    return handle;
  };

  readonly stop = (handle: number): void => {
    this.waiting.delete(handle);
  };

  /** Moves the clock on, running what falls due in the order it falls. */
  advance(milliseconds: number): void {
    const until = this.now + milliseconds;
    for (;;) {
      let due: [number, { readonly at: number; readonly work: () => void }] | undefined;
      for (const entry of this.waiting.entries()) {
        if (entry[1].at <= until && (due === undefined || entry[1].at < due[1].at)) due = entry;
      }
      if (due === undefined) break;
      const [handle, timer] = due;
      this.waiting.delete(handle);
      this.now = timer.at;
      timer.work();
    }
    this.now = until;
  }
}

/** A request the test answers when it likes. */
interface Call {
  readonly request: string;
  readonly answer: (value: string) => void;
  readonly fail: (reason: string) => void;
}

/** A scheduler over text requests, with the host replaced by one the test answers. */
function setup(delay = 100): {
  readonly latest: Latest<string, string>;
  readonly clock: Clock;
  readonly calls: Call[];
  readonly delivered: Array<{ readonly request: string; readonly outcome: Outcome<string> }>;
} {
  const clock = new Clock();
  const calls: Call[] = [];
  const delivered: Array<{ request: string; outcome: Outcome<string> }> = [];
  const latest = new Latest<string, string>({
    delay,
    keyOf: (request) => request,
    send: (request) =>
      new Promise<string>((resolve, reject) => {
        calls.push({
          request,
          answer: resolve,
          fail: (reason) => {
            reject(new Error(reason));
          },
        });
      }),
    deliver: (request, outcome) => {
      delivered.push({ request, outcome });
    },
    timers: clock,
  });
  return { latest, clock, calls, delivered };
}

/** Lets the promises that are already settled run their continuations. */
async function settled(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe('the requests the editor gives the host', () => {
  it('are sent once the delay has passed with no newer request, and not before', () => {
    const { latest, clock, calls } = setup();

    latest.submit('a');
    clock.advance(99);
    expect(calls.map((call) => call.request)).toEqual([]);
    clock.advance(1);

    expect(calls.map((call) => call.request)).toEqual(['a']);
  });

  it('send only the newest of those given within the delay', () => {
    const { latest, clock, calls } = setup();

    latest.submit('a');
    clock.advance(60);
    latest.submit('b');
    clock.advance(60);
    latest.submit('c');
    clock.advance(100);

    expect(calls.map((call) => call.request)).toEqual(['c']);
  });

  it('deliver what the host answered, with the request it answered', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);

    calls[0]?.answer('A');
    await settled();

    expect(delivered).toEqual([{ request: 'a', outcome: { ok: true, answer: 'A' } }]);
  });

  it('deliver a failure of the host as one', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);

    calls[0]?.fail('no host');
    await settled();

    expect(delivered).toHaveLength(1);
    expect(delivered[0]?.request).toBe('a');
    expect(delivered[0]?.outcome).toMatchObject({ ok: false, error: new Error('no host') });
  });

  it('drop the answer of a request that a newer one has replaced, and deliver the newer', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');

    // The older answer comes back after the newer request was given.
    calls[0]?.answer('A');
    await settled();
    expect(delivered).toEqual([]);
    clock.advance(100);
    calls[1]?.answer('B');
    await settled();

    expect(delivered).toEqual([{ request: 'b', outcome: { ok: true, answer: 'B' } }]);
  });

  it('drop the failure of a request that a newer one has replaced', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');

    calls[0]?.fail('too slow');
    await settled();

    expect(delivered).toEqual([]);
  });

  it('keep one request out at a time, and send the newest when it is back', async () => {
    const { latest, clock, calls } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');
    clock.advance(100);
    latest.submit('c');
    clock.advance(100);

    // `b` and `c` waited their turns: the host has been asked only for `a`.
    expect(calls.map((call) => call.request)).toEqual(['a']);
    calls[0]?.answer('A');
    await settled();

    expect(calls.map((call) => call.request)).toEqual(['a', 'c']);
  });

  it('do not ask again for what the newest request already asks', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);

    latest.submit('a');
    clock.advance(500);
    calls[0]?.answer('A');
    await settled();
    latest.submit('a');
    clock.advance(500);

    expect(calls.map((call) => call.request)).toEqual(['a']);
    expect(delivered).toHaveLength(1);
  });

  it('deliver the answer of a request that has become the newest again, without asking twice', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');
    latest.submit('a');

    calls[0]?.answer('A');
    await settled();
    clock.advance(500);

    expect(delivered).toEqual([{ request: 'a', outcome: { ok: true, answer: 'A' } }]);
    expect(calls.map((call) => call.request)).toEqual(['a']);
  });

  it('ask again for a request whose answer was dropped, when it is given once more', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');
    calls[0]?.answer('A');
    await settled();
    expect(delivered).toEqual([]);

    latest.submit('a');
    clock.advance(100);

    expect(calls.map((call) => call.request)).toEqual(['a', 'a']);
  });

  it('send and deliver nothing once stopped', async () => {
    const { latest, clock, calls, delivered } = setup();
    latest.submit('a');
    clock.advance(100);
    latest.submit('b');

    latest.stop();
    clock.advance(500);
    calls[0]?.answer('A');
    await settled();

    expect(calls.map((call) => call.request)).toEqual(['a']);
    expect(delivered).toEqual([]);
  });
});
