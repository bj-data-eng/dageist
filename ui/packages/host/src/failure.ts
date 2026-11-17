/**
 * @file How a failed command reaches the view. The host rejects a command
 * with its kind beside its message, and both hosts reject with a `HostError`
 * that carries the kind, so that a feature decides on the kind and never on
 * the text.
 */

import type { ErrorKind } from './contract';
import { ERROR_KINDS } from './contract';
import { DecodeError, ROOT, readField, readRecord, readString, readWord } from './decode';

/** A command the host answered with a failure, and what kind it is. */
export class HostError extends Error {
  /**
   * @param command The command that failed, by the name the host registers it under.
   * @param kind What the command ended in.
   * @param detail What went wrong, in words.
   */
  constructor(
    readonly command: string,
    readonly kind: ErrorKind,
    detail: string,
    options?: ErrorOptions,
  ) {
    super(`${command}: ${detail}`, options);
    this.name = 'HostError';
  }
}

/** A failure as the host sends it: its kind, and its message. */
export interface Failure {
  readonly kind: ErrorKind;
  readonly message: string;
}

const readFailure = readRecord(['kind', 'message'], (fields, path): Failure => ({
  kind: readField(fields, path, 'kind', readWord(ERROR_KINDS)),
  message: readField(fields, path, 'message', readString),
}));

/**
 * Reads what the host rejected a command with.
 *
 * @throws {DecodeError} When it is not a failure of the contract, saying where and why.
 */
export function decodeFailure(value: unknown): Failure {
  return readFailure(value, ROOT);
}

/** What `reason` says in words, when it says anything. */
function wordsOf(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === 'object' && reason !== null && 'message' in reason) {
    return String(reason.message);
  }
  return String(reason);
}

/**
 * Turns what a command rejected with into the `HostError` the view sees. A
 * failure of the contract keeps its kind. Anything else, such as the text
 * Tauri words itself for arguments it cannot read, is a `failed` command that
 * keeps what it said, and what was wrong with it as a failure.
 */
export function hostError(command: string, reason: unknown): HostError {
  if (reason instanceof Error || typeof reason !== 'object' || reason === null) {
    return new HostError(command, 'failed', wordsOf(reason), { cause: reason });
  }
  try {
    const { kind, message } = decodeFailure(reason);
    return new HostError(command, kind, message, { cause: reason });
  } catch (error) {
    if (!(error instanceof DecodeError)) throw error;
    return new HostError(command, 'failed', `${wordsOf(reason)} (${error.message})`, {
      cause: reason,
    });
  }
}
