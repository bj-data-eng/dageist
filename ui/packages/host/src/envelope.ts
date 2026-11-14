/**
 * @file How a message travels between the windows of the application. Every
 * host wraps what a window says in an envelope that names the window, and
 * leaves out the envelopes a window sent itself, so that no window ever
 * hears its own words, whatever carries them.
 */

/** Tells this window's messages from those of the other windows. */
const WINDOW_ID = Math.random().toString(36).slice(2);

/** A message as it travels between windows: who sent it, and what it says. */
interface Envelope {
  readonly from: string;
  readonly body: unknown;
}

function isEnvelope(value: unknown): value is Envelope {
  return (
    typeof value === 'object' &&
    value !== null &&
    'from' in value &&
    typeof value.from === 'string' &&
    'body' in value
  );
}

/** Returns `message` in an envelope that says this window sent it. */
export function sealMessage(message: unknown): unknown {
  const envelope: Envelope = { from: WINDOW_ID, body: message };
  return envelope;
}

/**
 * Calls `listener` with what `received` says when it is an envelope from
 * another window. One this window sent, or anything that is not an envelope,
 * is left out.
 */
export function deliverMessage(received: unknown, listener: (message: unknown) => void): void {
  if (isEnvelope(received) && received.from !== WINDOW_ID) listener(received.body);
}
