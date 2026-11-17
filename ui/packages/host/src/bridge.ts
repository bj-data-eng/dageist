/**
 * @file The host reached through the local bridge, which `dageist bridge`
 * serves to a page open in a plain browser. This is the only module in the
 * view that knows the bridge's transport: each command is a `POST` of its
 * arguments as JSON to `/bridge/<command>` on the page's own origin, which
 * the development server forwards to the bridge.
 */

import { browserWindows } from './browser-windows';
import type { BlockRequest, Host } from './contract';
import { DecodeError } from './decode';
import { decodeCatalogue } from './decode-catalogue';
import { decodeNode, decodeRows } from './decode-node';
import { decodeFits, decodeOffers, decodeOptions } from './decode-offers';
import { decodeRun } from './decode-run';
import { decodeBytes, decodeIdentity, decodeTable } from './decode-sample';
import { hostError } from './failure';

/** The path every command is under, on the page's own origin. */
const COMMAND_PATH = '/bridge/';

/** The command that starts the bridge, which the view tells a developer when it is not running. */
export const BRIDGE_START_COMMAND = 'cargo run --offline --locked -p dageist-tauri -- bridge';

/** The bridge cannot be reached: it is not running, or the page does not forward to it. */
export class BridgeUnavailableError extends Error {
  constructor(command: string, options?: ErrorOptions) {
    super(
      `${command}: the bridge is not running. Start it with \`${BRIDGE_START_COMMAND}\`, then reload the page.`,
      options,
    );
    this.name = 'BridgeUnavailableError';
  }
}

/** Returns what a failed command said, or `null` when the bridge did not say anything. */
async function readFailure(response: Response): Promise<unknown> {
  // Only the bridge answers a failure with JSON of its own. The development server answers one
  // for it, as plain text, when nothing listens, and that is not the bridge speaking.
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    return null;
  }
  try {
    return await response.json();
  } catch {
    // A body that is not JSON is not what the bridge sends.
    return null;
  }
}

/** Posts `command` with its arguments and gives the answer the bridge sent. */
async function post(command: string, args: Record<string, unknown>): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(`${COMMAND_PATH}${command}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args),
    });
  } catch (error) {
    throw new BridgeUnavailableError(command, { cause: error });
  }
  if (response.ok) return response;
  const failure = await readFailure(response);
  if (failure === null) throw new BridgeUnavailableError(command);
  throw hostError(command, failure);
}

/** Posts `command` and reads what it answered with `decode`, which refuses a shape the contract lacks. */
async function call<T>(
  command: string,
  decode: (value: unknown) => T,
  args: Record<string, unknown> = {},
): Promise<T> {
  const response = await post(command, args);
  let answer: unknown;
  try {
    answer = await response.json();
  } catch (error) {
    throw new Error(`${command}: the answer is not JSON`, { cause: error });
  }
  try {
    return decode(answer);
  } catch (error) {
    if (error instanceof DecodeError) {
      throw new Error(`${command}: ${error.message}`, { cause: error });
    }
    throw error;
  }
}

/** Returns the host that calls the bridge, with the windows of a plain browser. */
export function bridgeHost(): Host {
  return {
    coreIdentity: async () => call('core_identity', decodeIdentity),
    operationCatalogue: async () => call('operation_catalogue', decodeCatalogue),
    nodeInputs: async () => call('node_inputs', decodeNode),
    runDocument: async (document) => call('run_document', decodeRun, { document }),
    rowsAfterStep: async (document, step, limit) =>
      call('rows_after_step', decodeRows, { document, step, limit }),
    offersAt: async (place) =>
      call('offers_at', decodeOffers, { positions: place.positions, after: place.after }),
    candidateFits: async (request) =>
      call('candidate_fits', decodeFits, {
        start: request.start,
        argument: request.argument,
        candidates: request.candidates,
      }),
    optionOffers: async (entry, chosen) => call('option_offers', decodeOptions, { entry, chosen }),
    openSampleWindow: async (keys) => call('open_sample_window', decodeTable, { keys }),
    openSampleResult: async () => call('open_sample_result', decodeTable),
    fetchTableBlock: async (request: BlockRequest) => {
      const response = await post('table_block', {
        offset: request.offset,
        length: request.length,
        columns: request.columns,
      });
      try {
        return decodeBytes(await response.arrayBuffer());
      } catch (error) {
        if (error instanceof DecodeError) {
          throw new Error(`table_block: ${error.message}`, { cause: error });
        }
        throw error;
      }
    },
    ...browserWindows(),
  };
}
