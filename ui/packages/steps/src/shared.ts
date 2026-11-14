/**
 * @file The steps as every window of the application sees them. The main
 * window and the steps window each hold the list and the chosen step, and
 * tell each other of every change, so that what is done in one shows in the
 * other at once. A window that opens asks the others what they hold.
 *
 * This is storyboard code. In the real application the Rust core owns the
 * steps and tells every window of a change; nothing here is meant to survive.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { Host } from '@dageist/host';

import type { Step } from './model/steps';
import { VERBS } from './model/steps';
import { START } from './start';

/** The topic the windows talk about the steps on. */
const TOPIC = 'steps';
/** How often the steps window says it is there, in milliseconds. */
const BEAT = 1000;
/** How long after its last word the steps window counts as closed, in milliseconds. */
const SILENCE = 2500;

/** The steps and which of them is chosen. */
interface Held {
  readonly steps: readonly Step[];
  readonly selectedId: string | null;
}

/** What one window says to the others. */
type Message =
  | { readonly kind: 'hello' }
  | { readonly kind: 'here' }
  | { readonly kind: 'gone' }
  | ({ readonly kind: 'state' } & Held);

function isStep(value: unknown): value is Step {
  return (
    typeof value === 'object' &&
    value !== null &&
    'id' in value &&
    typeof value.id === 'string' &&
    'verb' in value &&
    VERBS.some((verb) => verb === value.verb)
  );
}

/** Returns `message` as something a window says about the steps, or `null` when it is not. */
function readMessage(message: unknown): Message | null {
  if (typeof message !== 'object' || message === null || !('kind' in message)) return null;
  if (message.kind === 'hello' || message.kind === 'here' || message.kind === 'gone') {
    return { kind: message.kind };
  }
  if (message.kind !== 'state' || !('steps' in message) || !('selectedId' in message)) return null;
  const { steps, selectedId } = message;
  if (!Array.isArray(steps) || !steps.every((step) => isStep(step))) return null;
  if (selectedId !== null && typeof selectedId !== 'string') return null;
  return { kind: 'state', steps, selectedId };
}

/** Which window is asking: the main one, or the one that shows the steps. */
export type Role = 'main' | 'steps';

/** The steps as a window holds them, and how it changes them. */
export interface SharedSteps extends Held {
  /** Whether the steps window is open, as far as the main window can tell. */
  readonly stepsOpen: boolean;
  /** Replaces the steps and the chosen step, here and in every other window. */
  readonly change: (steps: readonly Step[], selectedId: string | null) => void;
}

/** Holds the steps for one window and keeps them the same as in the others. */
export function useSharedSteps(host: Host, role: Role): SharedSteps {
  const [held, setHeld] = useState<Held>({ steps: START, selectedId: START[2]?.id ?? null });
  const [stepsOpen, setStepsOpen] = useState(false);
  // What is held now, for answering a window that asks after this one drew.
  const latest = useRef(held);
  const lastHeard = useRef(0);

  useEffect(() => {
    const say = (message: Message): void => {
      host.broadcast(TOPIC, message);
    };
    const stop = host.listen(TOPIC, (raw) => {
      const message = readMessage(raw);
      if (message === null) return;
      if (message.kind === 'state') {
        latest.current = { steps: message.steps, selectedId: message.selectedId };
        setHeld(latest.current);
      } else if (message.kind === 'hello') {
        say({ kind: 'state', ...latest.current });
      } else if (role === 'main') {
        lastHeard.current = message.kind === 'here' ? Date.now() : 0;
        setStepsOpen(message.kind === 'here');
      }
    });
    // Whoever is already open holds the steps as they are now.
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

  const change = useCallback(
    (steps: readonly Step[], selectedId: string | null) => {
      latest.current = { steps, selectedId };
      setHeld(latest.current);
      host.broadcast(TOPIC, { kind: 'state', steps, selectedId } satisfies Message);
    },
    [host],
  );

  return { ...held, stepsOpen, change };
}
