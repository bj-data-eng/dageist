/**
 * @file Asks the host for the newest request only. The editor asks the engine
 * to run its steps whenever they change, which can be many times a second, so
 * the requests are held for a moment and the newest of them is the one that
 * is sent; at most one request is out at a time; what comes back for a
 * request that a newer one has replaced is dropped, so the view never shows
 * an older answer over a newer edit; and a request that failed in a way that
 * may pass, as when the host is not there yet, is asked again after a while.
 */

/** What a request came to. */
export type Outcome<Answer> =
  | { readonly ok: true; readonly answer: Answer }
  | { readonly ok: false; readonly error: unknown };

/** The clock a scheduler waits with; a test gives its own. */
export interface Timers {
  readonly start: (work: () => void, milliseconds: number) => number;
  readonly stop: (handle: number) => void;
}

const BROWSER_TIMERS: Timers = {
  start: (work, milliseconds) => globalThis.setTimeout(work, milliseconds),
  stop: (handle) => {
    globalThis.clearTimeout(handle);
  },
};

/** What a scheduler needs. */
export interface LatestOptions<Request, Answer> {
  /** How long a request is held for a newer one to replace it, in milliseconds. */
  readonly delay: number;
  /** Tells requests that ask the same thing apart from the rest; the same text is the same request. */
  readonly keyOf: (request: Request) => string;
  /** Sends one request to the host. */
  readonly send: (request: Request) => Promise<Answer>;
  /** Receives what the newest request came to, and never what an older one did. */
  readonly deliver: (request: Request, outcome: Outcome<Answer>) => void;
  /**
   * Whether a failure may pass if the same request is asked again, as one
   * from a host that is not there yet may. A failure that is not, such as a
   * refusal the host gave on purpose, is the same each time and is not asked
   * again. No failure is by default.
   */
  readonly retryable?: (error: unknown) => boolean;
  /**
   * How long after a retryable failure the same request is asked again, while
   * it is still the newest, in milliseconds.
   */
  readonly retryAfter?: number;
  readonly timers?: Timers;
}

/** How long after a failure the request is asked again, unless the options say. */
const RETRY_AFTER = 2000;

/**
 * Sends the newest of the requests it is given: after `delay` has passed with
 * no newer one, and not while another is out. What comes back is delivered
 * only if no newer request has been given since.
 */
export class Latest<Request, Answer> {
  private readonly timers: Timers;
  /** The newest request given. */
  private newest: Request | null = null;
  /** The key of the newest request given. */
  private newestKey: string | null = null;
  /** The key of the request sent last, while its answer is still wanted. */
  private sentKey: string | null = null;
  private waiting: number | null = null;
  private out = false;
  /** Whether the delay ran out while a request was out. */
  private due = false;
  private stopped = false;

  constructor(private readonly options: LatestOptions<Request, Answer>) {
    this.timers = options.timers ?? BROWSER_TIMERS;
  }

  /** Gives a request. A request that asks what the newest already does is left out. */
  submit(request: Request): void {
    if (this.stopped) return;
    const key = this.options.keyOf(request);
    if (key === this.newestKey) return;
    this.newest = request;
    this.newestKey = key;
    this.due = false;
    if (this.waiting !== null) this.timers.stop(this.waiting);
    this.waiting = this.timers.start(() => {
      this.waiting = null;
      this.send();
    }, this.options.delay);
  }

  /** Stops: nothing is sent or delivered from now on. */
  stop(): void {
    this.stopped = true;
    if (this.waiting !== null) this.timers.stop(this.waiting);
    this.waiting = null;
  }

  /** Asks `key` again if it is still the newest and nothing is out. */
  private retry(key: string): void {
    if (this.stopped || this.out || key !== this.newestKey) return;
    this.send();
  }

  private send(): void {
    if (this.stopped || this.newest === null || this.newestKey === null) return;
    if (this.out) {
      // One request is out at a time; the newest goes when it is back.
      this.due = true;
      return;
    }
    // What is already out or answered for the newest is not asked again.
    if (this.newestKey === this.sentKey) return;
    const request = this.newest;
    const key = this.newestKey;
    this.out = true;
    this.sentKey = key;
    this.options.send(request).then(
      (answer) => {
        this.settle(request, key, { ok: true, answer });
      },
      (error: unknown) => {
        this.settle(request, key, { ok: false, error });
      },
    );
  }

  private settle(request: Request, key: string, outcome: Outcome<Answer>): void {
    this.out = false;
    if (this.stopped) return;
    if (key === this.newestKey) {
      this.options.deliver(request, outcome);
      if (!outcome.ok && (this.options.retryable?.(outcome.error) ?? false)) {
        // A failure that may pass is not an answer: ask again after a while, unless a newer
        // request comes first.
        this.sentKey = null;
        if (this.waiting === null) {
          this.waiting = this.timers.start(() => {
            this.waiting = null;
            this.retry(key);
          }, this.options.retryAfter ?? RETRY_AFTER);
        }
      }
    } else {
      // A newer request replaced it, so its answer is not wanted, and it may be asked again.
      this.sentKey = null;
    }
    if (this.due) {
      this.due = false;
      this.send();
    }
  }
}
