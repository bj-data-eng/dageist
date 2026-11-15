/**
 * @file The catalogue of operations the host serves, fetched once for a
 * window and handed to every part of the editor that needs a word from it.
 * While it loads, or if it cannot be had, the window says so in a plain line
 * instead of showing an editor that has nothing to offer.
 */

import { createContext, useContext, useEffect, useState } from 'react';

import { Panel } from '@dageist/design';
import type { Host } from '@dageist/host';

import type { Catalogue } from './model/catalogue';
import { indexCatalogue } from './model/catalogue';

/** What fetching the catalogue from `host` has come to. */
interface Fetched {
  readonly host: Host;
  readonly outcome:
    | { readonly state: 'failed'; readonly reason: string }
    | { readonly state: 'ready'; readonly catalogue: Catalogue };
}

const CatalogueContext = createContext<Catalogue | null>(null);

/** Returns the catalogue. It is only there below a `CatalogueProvider` that has it. */
export function useCatalogue(): Catalogue {
  const catalogue = useContext(CatalogueContext);
  if (catalogue === null) throw new Error('useCatalogue is used outside a CatalogueProvider.');
  return catalogue;
}

/** What the provider needs. */
export interface CatalogueProviderProps {
  readonly host: Host;
  /** What the window is called while the catalogue is not there. */
  readonly title: string;
  readonly children: React.ReactNode;
}

/** Fetches the catalogue from `host` and draws `children` once it is there. */
export function CatalogueProvider({
  host,
  title,
  children,
}: CatalogueProviderProps): React.JSX.Element {
  const [fetched, setFetched] = useState<Fetched | null>(null);

  useEffect(() => {
    let live = true;
    host
      .operationCatalogue()
      .then((view) => {
        if (!live) return;
        setFetched({ host, outcome: { state: 'ready', catalogue: indexCatalogue(view) } });
      })
      .catch((error: unknown) => {
        if (!live) return;
        setFetched({ host, outcome: { state: 'failed', reason: String(error) } });
      });
    return () => {
      live = false;
    };
  }, [host]);

  // What was fetched from another host is not what this one serves.
  const outcome = fetched?.host === host ? fetched.outcome : null;
  if (outcome?.state === 'ready') {
    return (
      <CatalogueContext.Provider value={outcome.catalogue}>{children}</CatalogueContext.Provider>
    );
  }
  const line =
    outcome === null
      ? 'Loading the catalogue of operations…'
      : `The catalogue of operations could not be loaded: ${outcome.reason}`;
  return (
    <Panel mark="ƒx" title={title} details={[line]}>
      {null}
    </Panel>
  );
}
