import { afterEach, describe, expect, it, vi } from 'vitest';

import { BRIDGE_START_COMMAND, BridgeUnavailableError, bridgeHost } from './bridge';
import fitsFixture from '../fixtures/fits-argument.json';
import nodeFixture from '../fixtures/node-inputs.json';
import offersFixture from '../fixtures/offers-value.json';
import optionsFixture from '../fixtures/options.json';
import typedFixture from '../fixtures/typed-expression.json';
import reportFixture from '../fixtures/step-report.json';
import rowsFixture from '../fixtures/step-rows.json';
import { withField } from '../test-support/tamper';

/** What the page sent to `fetch`: where, and with what. */
interface Sent {
  readonly path: string;
  readonly method: string | undefined;
  readonly contentType: string | null;
  readonly body: unknown;
}

const sent: Sent[] = [];

/** Makes `fetch` answer with `answer`, and records what it was asked. */
function bridgeAnswers(answer: () => Response | Promise<Response>): void {
  sent.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      sent.push({
        path: input instanceof Request ? input.url : String(input),
        method: init?.method,
        contentType: new Headers(init?.headers).get('content-type'),
        body: typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : init?.body,
      });
      return Promise.resolve(answer());
    }),
  );
}

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the host that calls the bridge', () => {
  it('posts the arguments of a command as JSON to its path and decodes the answer', async () => {
    bridgeAnswers(() => jsonResponse(reportFixture));

    const run = await bridgeHost().runDocument('{"version":1,"steps":[]}');

    expect(run.status).toBe('run');
    expect(sent).toEqual([
      {
        path: '/bridge/run_document',
        method: 'POST',
        contentType: 'application/json',
        body: { document: '{"version":1,"steps":[]}' },
      },
    ]);
  });

  it('sends an empty object for a command that takes no arguments', async () => {
    bridgeAnswers(() => jsonResponse(nodeFixture));

    const node = await bridgeHost().nodeInputs();

    expect(node.source.name).toBe('claims');
    expect(sent[0]).toMatchObject({ path: '/bridge/node_inputs', body: {} });
  });

  it('names the arguments of the offers commands as the host does', async () => {
    const place = {
      positions: [{ kind: 'outer', name: 'select' }],
      after: { kind: 'value', dtype: 'Datetime(us)', shape: 'many' },
    } as const;
    bridgeAnswers(() => jsonResponse(offersFixture));
    const offers = await bridgeHost().offersAt(place);
    bridgeAnswers(() => jsonResponse(fitsFixture));
    const request = {
      start: null,
      argument: { entry: 'Expr.dt.offset_by', parameter: 'by', place },
      candidates: [{ kind: 'field', dtype: 'String' }],
    } as const;
    const fits = await bridgeHost().candidateFits(request);
    const fitsSent = [...sent];
    bridgeAnswers(() => jsonResponse(optionsFixture));
    const chosen = [{ parameter: 'how', word: 'cross' }];
    const found = await bridgeHost().optionOffers('LazyFrame.join', chosen);

    expect(offers.entries).toHaveLength(35);
    expect(fits.fits).toHaveLength(3);
    expect(found.options.length).toBeGreaterThan(0);
    expect(fitsSent[0]).toMatchObject({
      path: '/bridge/candidate_fits',
      body: request,
    });
    expect(sent[0]).toMatchObject({
      path: '/bridge/option_offers',
      body: { entry: 'LazyFrame.join', chosen },
    });
  });

  it('sends a typed text with what it is typed into and the slot it stands in', async () => {
    bridgeAnswers(() => jsonResponse(typedFixture));
    const request = {
      text: '1.5',
      typed: { kind: 'expression' },
      start: null,
      argument: {
        entry: 'Expr.dt.offset_by',
        parameter: 'by',
        place: {
          positions: [{ kind: 'outer', name: 'select' }],
          after: { kind: 'value', dtype: 'Datetime(us)', shape: 'many' },
        },
      },
    } as const;

    const read = await bridgeHost().readTyped(request);

    expect(read.readings).toHaveLength(2);
    expect(sent[0]).toMatchObject({ path: '/bridge/read_typed', body: request });
  });

  it('sends the place of the offers as the positions and what the expression has', async () => {
    bridgeAnswers(() => jsonResponse(offersFixture));
    const place = {
      positions: [{ kind: 'outer', name: 'select' }],
      after: { kind: 'start' },
    } as const;
    await bridgeHost().offersAt(place);

    expect(sent[0]).toMatchObject({ path: '/bridge/offers_at', body: place });
  });

  it('names the arguments of the rows and the sample commands as the host does', async () => {
    bridgeAnswers(() => jsonResponse(rowsFixture));
    await bridgeHost().rowsAfterStep('{}', 'days_open', 7);
    bridgeAnswers(() => jsonResponse({ rows: 3, columns: [{ name: 'dcn', dataType: 'str' }] }));
    const table = await bridgeHost().openSampleWindow(10);

    expect(table).toEqual({ rows: 3, columns: [{ name: 'dcn', dataType: 'str' }] });
    expect(sent[0]).toMatchObject({ path: '/bridge/open_sample_window', body: { keys: 10 } });
  });

  it('asks for a block with the rows and the columns and hands over its bytes', async () => {
    bridgeAnswers(
      () =>
        new Response(new Uint8Array([0xff, 0xff, 0xff, 0xff]), {
          headers: { 'Content-Type': 'application/vnd.apache.arrow.stream' },
        }),
    );

    const bytes = await bridgeHost().fetchTableBlock({
      offset: 3,
      length: 7,
      columns: ['dcn'],
    });

    expect(new Uint8Array(bytes)).toEqual(new Uint8Array([0xff, 0xff, 0xff, 0xff]));
    expect(sent[0]).toMatchObject({
      path: '/bridge/table_block',
      body: { offset: 3, length: 7, columns: ['dcn'] },
    });
  });

  it('rejects with the message a failed command sent, under its name', async () => {
    bridgeAnswers(() =>
      jsonResponse({ kind: 'limit', message: 'a preview holds at most 1000 rows' }, 422),
    );

    await expect(bridgeHost().rowsAfterStep('{}', 'a', 5000)).rejects.toThrow(
      'rows_after_step: a preview holds at most 1000 rows',
    );
  });

  it('refuses an answer of a shape the contract lacks, with its place', async () => {
    bridgeAnswers(() => jsonResponse(withField(nodeFixture, [], 'unexpected', 1)));

    await expect(bridgeHost().nodeInputs()).rejects.toThrow(/node_inputs: \$/u);
  });

  it('refuses an answer that is not JSON', async () => {
    bridgeAnswers(() => new Response('<html></html>', { status: 200 }));

    await expect(bridgeHost().nodeInputs()).rejects.toThrow('node_inputs: the answer is not JSON');
  });

  it('says how to start the bridge when nothing can be reached', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.reject(new TypeError('Failed to fetch'))),
    );

    const failure = await bridgeHost()
      .coreIdentity()
      .then(
        () => null,
        (error: unknown) => error,
      );

    expect(failure).toBeInstanceOf(BridgeUnavailableError);
    expect(String(failure instanceof Error ? failure.message : failure)).toContain(
      BRIDGE_START_COMMAND,
    );
  });

  it('says how to start the bridge when the development server cannot forward to it', async () => {
    // The development server answers a refused connection to the bridge with plain text.
    bridgeAnswers(
      () => new Response('', { status: 500, headers: { 'Content-Type': 'text/plain' } }),
    );

    await expect(bridgeHost().operationCatalogue()).rejects.toBeInstanceOf(BridgeUnavailableError);
  });
});
