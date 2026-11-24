/**
 * @file The host reached through Tauri's IPC. This is the only module in the
 * view that knows Tauri exists.
 */

import type { BlockRequest, Host } from './contract';
import { DecodeError } from './decode';
import { decodeCatalogue } from './decode-catalogue';
import { decodeNode, decodeRows } from './decode-node';
import {
  decodeFits,
  decodeOffers,
  decodeOptions,
  decodeReadings,
  decodeStepStarts,
} from './decode-offers';
import { decodeDocumentState, decodeEditAnswer } from './decode-document';
import { decodeBlock } from './decode-block';
import { decodeIdentity, decodeTable } from './decode-sample';
import { deliverMessage, sealMessage } from './envelope';
import { hostError } from './failure';

/** Returns the Tauri host, or `null` when the page is open in a plain browser. */
export function tauriHost(): Host | null {
  const found = window.__TAURI__;
  if (found === undefined) return null;
  const { core, event: events } = found;

  /** Invokes `command` and gives what it answered, which is not yet known to be anything. */
  async function invoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
    try {
      return await core.invoke<unknown>(command, args);
    } catch (error) {
      throw hostError(command, error);
    }
  }

  /** Invokes `command` and reads what it answered with `decode`, which refuses a shape the contract lacks. */
  async function invokeDecoded<T>(
    command: string,
    decode: (value: unknown) => T,
    args?: Record<string, unknown>,
  ): Promise<T> {
    const answer = await invoke(command, args);
    try {
      return decode(answer);
    } catch (error) {
      if (error instanceof DecodeError) throw hostError(command, error);
      throw error;
    }
  }

  return {
    coreIdentity: async () => invokeDecoded('core_identity', decodeIdentity),
    operationCatalogue: async () => invokeDecoded('operation_catalogue', decodeCatalogue),
    nodeInputs: async () => invokeDecoded('node_inputs', decodeNode),
    nodeDocument: async () => invokeDecoded('node_document', decodeDocumentState),
    editDocument: async (revision, edit) =>
      invokeDecoded('edit_document', decodeEditAnswer, { revision, edit }),
    rowsAfterStep: async (revision, step, limit) =>
      invokeDecoded('rows_after_step', decodeRows, { revision, step, limit }),
    offersAt: async (place) =>
      invokeDecoded('offers_at', decodeOffers, { positions: place.positions, after: place.after }),
    candidateFits: async (request) =>
      invokeDecoded('candidate_fits', decodeFits, {
        start: request.start,
        argument: request.argument,
        candidates: request.candidates,
      }),
    readTyped: async (request) =>
      invokeDecoded('read_typed', decodeReadings, {
        text: request.text,
        typed: request.typed,
        start: request.start,
        argument: request.argument,
      }),
    optionOffers: async (entry, chosen) =>
      invokeDecoded('option_offers', decodeOptions, { entry, chosen }),
    stepStarts: async (request) =>
      invokeDecoded('step_starts', decodeStepStarts, {
        columns: request.columns,
        tables: request.tables,
      }),
    openSampleWindow: async (keys) => invokeDecoded('open_sample_window', decodeTable, { keys }),
    openSampleResult: async () => invokeDecoded('open_sample_result', decodeTable),
    fetchTableBlock: async (request: BlockRequest) =>
      invokeDecoded('table_block', (answer) => decodeBlock(answer, request.columns), {
        offset: request.offset,
        length: request.length,
        columns: request.columns,
      }),
    openWindow: async (name) => {
      // The command answers nothing, so there is nothing to read.
      await invoke('open_view_window', { name });
    },
    broadcast: (topic, message) => {
      // A message that cannot be sent is one the other window never sees; it asks again.
      void events.emit(`dageist:${topic}`, sealMessage(message)).catch(() => undefined);
    },
    listen: (topic, listener) => {
      // Tauri delivers an event to the window that sent it too; the envelope leaves those out.
      const stop = events.listen<unknown>(`dageist:${topic}`, ({ payload }) => {
        deliverMessage(payload, listener);
      });
      return () => {
        void stop.then((unlisten) => {
          unlisten();
        });
      };
    },
  };
}
