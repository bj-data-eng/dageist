/**
 * @file Which step is chosen, as every window of the application sees it. The main window and the
 * steps window each hold the chosen step, and tell each other of every change, so that what is
 * chosen in one shows in the other at once. A window that opens asks the others what they hold.
 * The document itself is not shared here: the host holds it, and each window asks the host.
 *
 * The main window tells whether the steps window is open by its regular word, which a browser may
 * slow down for a window in the background, so that sign can lag behind.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { Host } from '@dageist/host';

/** The topic the windows talk about the chosen step on. */
const TOPIC = 'selection';
/** How often the steps window says it is there, in milliseconds. */
const BEAT = 1000;
/** How long after its last word the steps window counts as closed, in milliseconds. */
const SILENCE = 2500;

/** What one window says to the others. */
type Message =
  | { readonly kind: 'hello' }
  | { readonly kind: 'here' }
  | { readonly kind: 'gone' }
  | { readonly kind: 'chosen'; readonly id: string | null };

/** Returns `message` as something a window says about the chosen step, or `null` when it is not. */
function readMessage(message: unknown): Message | null {
  if (typeof message !== 'object' || message === null || !('kind' in message)) return null;
  if (message.kind === 'hello' || message.kind === 'here' || message.kind === 'gone') {
    return { kind: message.kind };
  }
  if (message.kind !== 'chosen' || !('id' in message)) return null;
  const { id } = message;
  return typeof id === 'string' || id === null ? { kind: 'chosen', id } : null;
}

/** Which window is asking: the main one, or the one that shows the steps. */
export type Role = 'main' | 'steps';

/** The chosen step as a window holds it, and how it is changed. */
export interface Selection {
  /** The name of the chosen step, or `null` when none is. */
  readonly selectedId: string | null;
  /** Whether the steps window is open, as far as the main window can tell. */
  readonly stepsOpen: boolean;
  /** Chooses the step called `id`, here and in every other window. */
  readonly choose: (id: string | null) => void;
}

/** Holds the chosen step for one window and keeps it the same as in the others. */
export function useSelection(host: Host, role: Role): Selection {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [stepsOpen, setStepsOpen] = useState(false);
  // What is held now, for answering a window that asks after this one chose.
  const latest = useRef<string | null>(null);
  const lastHeard = useRef(0);

  useEffect(() => {
    const say = (message: Message): void => {
      host.broadcast(TOPIC, message);
    };
    const stop = host.listen(TOPIC, (raw) => {
      const message = readMessage(raw);
      if (message === null) return;
      if (message.kind === 'chosen') {
        latest.current = message.id;
        setSelectedId(message.id);
      } else if (message.kind === 'hello') {
        say({ kind: 'chosen', id: latest.current });
      } else if (role === 'main') {
        lastHeard.current = message.kind === 'here' ? Date.now() : 0;
        setStepsOpen(message.kind === 'here');
      }
    });
    // Whoever is already open holds the chosen step.
    say({ kind: 'hello' });
    if (role === 'steps') say({ kind: 'here' });
    const beat = window.setInterval(() => {
      if (role === 'steps') say({ kind: 'here' });
      else setStepsOpen(Date.now() - lastHeard.current < SILENCE);
    }, BEAT);
    const leave = (): void => {
      if (role === 'steps') say({ kind: 'gone' });
    };
    window.addEventListener('pagehide', leave);
    return () => {
      window.removeEventListener('pagehide', leave);
      window.clearInterval(beat);
      stop();
    };
  }, [host, role]);

  const choose = useCallback(
    (id: string | null) => {
      latest.current = id;
      setSelectedId(id);
      host.broadcast(TOPIC, { kind: 'chosen', id } satisfies Message);
    },
    [host],
  );

  return { selectedId, stepsOpen, choose };
}
