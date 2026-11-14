/**
 * @file A stand-in host for running the view in a plain browser, where there
 * is no engine. It makes up rows so that features can be worked on without
 * building the application. It is loaded only when Tauri is absent.
 */

import { tableFromArrays, tableToIPC } from '@uwdata/flechette';

import type { BlockRequest, ColumnView, Host, TableView } from '@dageist/host';

const COLUMNS: readonly ColumnView[] = [
  { name: 'dcn', dataType: 'str' },
  ...Array.from({ length: 39 }, (_unused, index) => ({
    name: `measure_${String(index + 1).padStart(3, '0')}`,
    dataType: 'i32',
  })),
];

/** The made-up row counts of the two ways of opening the sample. */
const WINDOW_ROWS = 2552;
const RESULT_ROWS = 5_000_000;

function makeBlock(request: BlockRequest, rows: number): ArrayBuffer {
  const count = Math.max(0, Math.min(request.length, rows - request.offset));
  const data: Record<string, Array<string | number>> = {};
  for (const name of request.columns) {
    data[name] = Array.from({ length: count }, (_unused, at) => {
      const row = request.offset + at;
      return name === 'dcn' ? `DCN-${String(10_000_000 + row)}` : row % 1000;
    });
  }
  const bytes = tableToIPC(tableFromArrays(data), { format: 'stream' });
  if (bytes === null) throw new Error('The stand-in host could not encode a block.');
  return bytes.slice().buffer;
}

/** Returns a host that answers from made-up data. */
export function demoHost(): Host {
  let rows = 0;
  const open = (count: number): TableView => {
    rows = count;
    return { rows, columns: COLUMNS };
  };
  return {
    coreIdentity: async () => Promise.resolve({ name: 'Browser preview', version: 'made-up data' }),
    openSampleWindow: async () => Promise.resolve(open(WINDOW_ROWS)),
    openSampleResult: async () => Promise.resolve(open(RESULT_ROWS)),
    fetchTableBlock: async (request) => Promise.resolve(makeBlock(request, rows)),
    openWindow: async (name) => {
      // A browser tab stands in for the host's window. Opening it by name again goes back to it.
      const address = new URL(window.location.href);
      address.search = `?window=${name}`;
      address.hash = '';
      window.open(address, `dageist-${name}`)?.focus();
      return Promise.resolve();
    },
    broadcast: (topic, message) => {
      const channel = new BroadcastChannel(`dageist:${topic}`);
      // oxlint-disable-next-line unicorn/require-post-message-target-origin -- a broadcast channel takes no target; it reaches this origin only
      channel.postMessage(message);
      channel.close();
    },
    listen: (topic, listener) => {
      // A channel does not hear what its own window says on it.
      const channel = new BroadcastChannel(`dageist:${topic}`);
      channel.addEventListener('message', (event: MessageEvent<unknown>) => {
        listener(event.data);
      });
      return () => {
        channel.close();
      };
    },
  };
}
