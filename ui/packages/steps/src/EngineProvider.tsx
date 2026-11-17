/**
 * @file What the editor is made of, fetched once for a window and handed to
 * every part of it that needs it: the catalogue of operations the host
 * serves, and what the node is connected to. While they load, or if they
 * cannot be had, the window says so in a plain line instead of showing an
 * editor that has nothing to offer.
 */

import { createContext, useContext, useEffect, useState } from 'react';

import { Panel } from '@dageist/design';
import type { Host, NodeView } from '@dageist/host';

import type { Catalogue } from './model/catalogue';
import { indexCatalogue } from './model/catalogue';

/** What the editor is made of. */
interface Parts {
  readonly catalogue: Catalogue;
  readonly node: NodeView;
}

/** What fetching the parts from `host` has come to. */
interface Fetched {
  readonly host: Host;
  readonly outcome:
    | { readonly state: 'failed'; readonly reason: string }
    | { readonly state: 'ready'; readonly parts: Parts };
}

const PartsContext = createContext<Parts | null>(null);

function useParts(): Parts {
  const parts = useContext(PartsContext);
  if (parts === null) throw new Error('The editor is used outside an EngineProvider.');
  return parts;
}

/** Returns the catalogue. It is only there below an `EngineProvider` that has it. */
export function useCatalogue(): Catalogue {
  return useParts().catalogue;
}

/** Returns what the node is connected to. It is only there below an `EngineProvider` that has it. */
export function useNode(): NodeView {
  return useParts().node;
}

/** What the provider needs. */
export interface EngineProviderProps {
  readonly host: Host;
  /** What the window is called while the parts are not there. */
  readonly title: string;
  readonly children: React.ReactNode;
}

/** Fetches the catalogue and the node from `host` and draws `children` once they are there. */
export function EngineProvider({ host, title, children }: EngineProviderProps): React.JSX.Element {
  const [fetched, setFetched] = useState<Fetched | null>(null);

  useEffect(() => {
    let live = true;
    Promise.all([host.operationCatalogue(), host.nodeInputs()])
      .then(([view, node]) => {
        if (!live) return;
        setFetched({
          host,
          outcome: { state: 'ready', parts: { catalogue: indexCatalogue(view), node } },
        });
      })
      .catch((error: unknown) => {
        if (!live) return;
        setFetched({
          host,
          outcome: {
            state: 'failed',
            reason: error instanceof Error ? error.message : String(error),
          },
        });
      });
    return () => {
      live = false;
    };
  }, [host]);

  // What was fetched from another host is not what this one serves.
  const outcome = fetched?.host === host ? fetched.outcome : null;
  if (outcome?.state === 'ready') {
    return <PartsContext.Provider value={outcome.parts}>{children}</PartsContext.Provider>;
  }
  const line =
    outcome === null
      ? 'Loading the catalogue of operations and the inputs…'
      : `The catalogue of operations and the inputs could not be loaded: ${outcome.reason}`;
  return (
    <Panel mark="ƒx" title={title} details={[line]}>
      {null}
    </Panel>
  );
}
