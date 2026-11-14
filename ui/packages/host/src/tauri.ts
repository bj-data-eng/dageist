/**
 * @file The host reached through Tauri's IPC. This is the only module in the
 * view that knows Tauri exists.
 */

import type { BlockRequest, Host, IdentityView, TableView } from './contract';
import { deliverMessage, sealMessage } from './envelope';

/** Turns what a failed command rejected with into an `Error`. */
function commandError(command: string, reason: unknown): Error {
  let detail = String(reason);
  if (reason instanceof Error) {
    detail = reason.message;
  } else if (typeof reason === 'object' && reason !== null && 'message' in reason) {
    detail = String(reason.message);
  }
  return new Error(`${command}: ${detail}`, { cause: reason });
}

/** Returns the Tauri host, or `null` when the page is open in a plain browser. */
export function tauriHost(): Host | null {
  const found = window.__TAURI__;
  if (found === undefined) return null;
  const { core, event: events } = found;

  async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return await core.invoke<T>(command, args);
    } catch (error) {
      throw commandError(command, error);
    }
  }

  return {
    coreIdentity: async () => invoke<IdentityView>('core_identity'),
    openSampleWindow: async (keys) => invoke<TableView>('open_sample_window', { keys }),
    openSampleResult: async () => invoke<TableView>('open_sample_result'),
    fetchTableBlock: async (request: BlockRequest) =>
      invoke<ArrayBuffer>('table_block', {
        offset: request.offset,
        length: request.length,
        columns: request.columns,
      }),
    openWindow: async (name) => invoke<undefined>('open_view_window', { name }),
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
