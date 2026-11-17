/**
 * @file The windows of the application as a plain browser has them: tabs, and
 * the messages between them over a broadcast channel. A browser has no host
 * to open windows, so the page does it itself. Messages travel in the same
 * envelope the Tauri host uses, so no tab hears its own words.
 */

import type { Host } from './contract';
import { deliverMessage, sealMessage } from './envelope';

/** What a host does with windows and messages between them. */
export type BrowserWindows = Pick<Host, 'openWindow' | 'broadcast' | 'listen'>;

/** Returns the window commands of a page open in a plain browser. */
export function browserWindows(): BrowserWindows {
  return {
    openWindow: async (name) => {
      // A browser tab stands in for the host's window. Asking for the tab by name with no
      // address returns the open one as it is, or a new blank one, which is then sent to the
      // view. Giving the address every time would load the open tab again.
      const tab = window.open('', `dageist-${name}`);
      if (tab === null) return Promise.resolve();
      if (tab.location.href === 'about:blank') {
        const address = new URL(window.location.href);
        address.search = `?window=${name}`;
        address.hash = '';
        tab.location.replace(address);
      }
      tab.focus();
      return Promise.resolve();
    },
    broadcast: (topic, message) => {
      const channel = new BroadcastChannel(`dageist:${topic}`);
      // oxlint-disable-next-line unicorn/require-post-message-target-origin -- a broadcast channel takes no target; it reaches this origin only
      channel.postMessage(sealMessage(message));
      channel.close();
    },
    listen: (topic, listener) => {
      // Another channel of the same name in this window hears what is sent here too; the
      // envelope leaves this window's own messages out, as it does for the Tauri host.
      const channel = new BroadcastChannel(`dageist:${topic}`);
      channel.addEventListener('message', (event: MessageEvent<unknown>) => {
        deliverMessage(event.data, listener);
      });
      return () => {
        channel.close();
      };
    },
  };
}
