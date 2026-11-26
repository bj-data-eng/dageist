import { afterEach, describe, expect, it, vi } from 'vitest';

import { bridgeHost } from './bridge';
import { tauriHost } from './tauri';

// The host rejects a failed command with its kind beside its message. The view decides on the
// kind, so both hosts hand it over the same way, whatever carries the failure.

const KINDS = ['noSuchStep', 'notRun', 'refused', 'limit', 'stale', 'failed'] as const;

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** Makes the bridge answer every command with `answer`. */
function bridgeAnswers(answer: () => Response): void {
  vi.stubGlobal(
    'fetch',
    vi.fn(() => Promise.resolve(answer())),
  );
}

/** Makes `window.__TAURI__` reject every command with `reason`. */
function tauriRejects(reason: unknown): void {
  vi.stubGlobal('window', {
    __TAURI__: {
      // Tauri rejects with whatever the command returned, which is not an Error.
      // oxlint-disable-next-line typescript/prefer-promise-reject-errors
      core: { invoke: vi.fn(() => Promise.reject(reason)) },
      event: { emit: vi.fn(), listen: vi.fn() },
    },
  });
}

/** What a promise rejected with. */
async function rejection(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => {
      throw new Error('the command was expected to fail');
    },
    (error: unknown) => error,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe.each(KINDS)('a command that fails with the kind %s', (kind) => {
  it('rejects through the bridge with that kind and the message, under its name', async () => {
    bridgeAnswers(() => jsonResponse({ kind, message: 'the reason' }, 422));

    const error = await rejection(bridgeHost().rowsAfterStep('document-timing', 1, 'a', 5));

    expect(error).toBeInstanceOf(Error);
    expect(error).toMatchObject({
      kind,
      message: 'rows_after_step: the reason',
    });
  });

  it('rejects through Tauri with that kind and the message, under its name', async () => {
    tauriRejects({ kind, message: 'the reason' });
    const host = tauriHost();

    const error = await rejection(
      host?.rowsAfterStep('document-timing', 1, 'a', 5) ?? Promise.resolve(),
    );

    expect(error).toBeInstanceOf(Error);
    expect(error).toMatchObject({
      kind,
      message: 'rows_after_step: the reason',
    });
  });
});

describe('a failure whose kind is not one the contract names', () => {
  it('is a failed command through the bridge, and says what was wrong with it', async () => {
    bridgeAnswers(() => jsonResponse({ kind: 'exploded', message: 'the reason' }, 422));

    const error = await rejection(bridgeHost().rowsAfterStep('document-timing', 1, 'a', 5));

    expect(error).toMatchObject({ kind: 'failed' });
    expect(String(error instanceof Error ? error.message : error)).toContain('exploded');
  });

  it('is a failed command through Tauri, and says what was wrong with it', async () => {
    tauriRejects({ kind: 'exploded', message: 'the reason' });

    const error = await rejection(
      tauriHost()?.rowsAfterStep('document-timing', 1, 'a', 5) ?? Promise.resolve(),
    );

    expect(error).toMatchObject({ kind: 'failed' });
    expect(String(error instanceof Error ? error.message : error)).toContain('exploded');
  });
});

describe('a failure that Tauri words itself', () => {
  it('is a failed command that keeps its text', async () => {
    tauriRejects('invalid args `limit` for command `rows_after_step`');

    const error = await rejection(
      tauriHost()?.rowsAfterStep('document-timing', 1, 'a', 5) ?? Promise.resolve(),
    );

    expect(error).toMatchObject({
      kind: 'failed',
      message: 'rows_after_step: invalid args `limit` for command `rows_after_step`',
    });
  });
});
