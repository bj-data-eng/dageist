/**
 * @file The shell: the composition root that decides which host the view
 * talks to and which feature is on screen. Features never import each other;
 * they meet only here.
 */

import { useEffect, useState } from 'react';

import { Panel, ToggleButton, cx } from '@dageist/design';
import type { Host } from '@dageist/host';
import { tauriHost } from '@dageist/host';
import { StepsWindow, StepsWorkspace } from '@dageist/steps';
import { TableViewer } from '@dageist/table';

/** A feature the shell can put on screen. */
type Workspace = 'steps' | 'table' | 'pieces';

/** A workspace and what its tab says. */
interface WorkspaceTab {
  readonly id: Workspace;
  readonly label: string;
}

const WORKSPACES: readonly WorkspaceTab[] = [
  { id: 'steps', label: 'Transform' },
  { id: 'table', label: 'Table' },
  { id: 'pieces', label: 'Pieces' },
];

/**
 * Returns the workspace the address names after its `#`, or the step editor.
 * The address keeps the workspace, so reloading the page stays on it.
 */
function startingWorkspace(): Workspace {
  const named = window.location.hash.slice(1);
  return WORKSPACES.find((entry) => entry.id === named)?.id ?? 'steps';
}

/** What the status badge says about the host. */
interface HostStatus {
  readonly text: string;
  readonly tone: 'pending' | 'ready' | 'failed';
}

/** The token Tauri replaces with a style nonce when it serves the page. */
const NONCE_PLACEHOLDER = '__TAURI_STYLE_NONCE__';

/**
 * Returns the nonce the content security policy accepts for injected styles,
 * or `null` in a plain browser, where there is no policy and no nonce.
 */
function readStyleNonce(): string | null {
  const content = document.querySelector('meta[name="style-nonce"]')?.getAttribute('content');
  return content === null || content === undefined || content === NONCE_PLACEHOLDER
    ? null
    : content;
}

/** Returns the host to use: Tauri when it is there, a stand-in otherwise. */
async function findHost(): Promise<Host> {
  const tauri = tauriHost();
  if (tauri !== null) return tauri;
  const { demoHost } = await import('./demo-host');
  return demoHost();
}

const STYLE_NONCE = readStyleNonce();

/**
 * Whether this page is the steps window and not the main one. The address
 * says so after its `?`, which is how the host opens that window.
 */
const IS_STEPS_WINDOW = new URLSearchParams(window.location.search).get('window') === 'steps';

/** Draws the application window. */
export function App(): React.JSX.Element {
  const [host, setHost] = useState<Host | null>(null);
  const [status, setStatus] = useState<HostStatus>({
    text: 'Connecting to core…',
    tone: 'pending',
  });
  const [workspace, setWorkspace] = useState<Workspace>(startingWorkspace);
  // A workspace is mounted when it is first shown and then stays mounted, so
  // switching away and back keeps what was being done in it.
  const [visited, setVisited] = useState<readonly Workspace[]>(() => [startingWorkspace()]);

  useEffect(() => {
    if (IS_STEPS_WINDOW) document.title = 'dageist · steps';
  }, []);

  useEffect(() => {
    let live = true;
    const connect = async (): Promise<void> => {
      try {
        const found = await findHost();
        // The host opens windows and carries messages whether or not the core answers.
        if (live) setHost(found);
        const identity = await found.coreIdentity();
        if (!live) return;
        setStatus({ text: `${identity.name} ${identity.version}`, tone: 'ready' });
      } catch (error) {
        if (live) setStatus({ text: `Core unavailable: ${String(error)}`, tone: 'failed' });
      }
    };
    void connect();
    return () => {
      live = false;
    };
  }, []);

  const show = (next: Workspace): void => {
    setWorkspace(next);
    window.history.replaceState(null, '', `#${next}`);
    if (!visited.includes(next)) setVisited([...visited, next]);
  };
  // A workspace that is not on screen keeps its box, collapsed to no height,
  // because a box that is removed from layout forgets how far it was scrolled.
  const frame = (id: Workspace): string =>
    cx(
      'flex min-h-0 justify-center',
      id === workspace ? 'flex-1' : 'invisible h-0 overflow-hidden',
    );

  if (IS_STEPS_WINDOW) {
    return (
      <main className="flex h-full flex-col bg-canvas bg-dots p-4">
        {host === null ? (
          <Panel mark="ƒx" title="Steps" details={[status.text]}>
            {null}
          </Panel>
        ) : (
          <StepsWindow host={host} styleNonce={STYLE_NONCE} />
        )}
      </main>
    );
  }

  return (
    <div className="flex h-full flex-col bg-canvas bg-dots">
      <nav aria-label="Workspaces" className="flex items-center gap-1.5 px-7 pt-3.5">
        {WORKSPACES.map((entry) => (
          <ToggleButton
            key={entry.id}
            pressed={entry.id === workspace}
            disabled={false}
            onPress={() => {
              show(entry.id);
            }}
          >
            {entry.label}
          </ToggleButton>
        ))}
        <output className="ml-auto flex items-center gap-2 rounded-lg border border-line bg-panel px-3 py-1 text-xs text-muted">
          <span
            aria-hidden="true"
            className={cx(
              'size-1.5 rounded-full',
              status.tone === 'ready' && 'bg-ready',
              status.tone === 'failed' && 'bg-danger',
              status.tone === 'pending' && 'bg-muted',
            )}
          />
          {status.text}
        </output>
      </nav>
      <main className="flex min-h-0 flex-1 flex-col px-7 pt-3 pb-6">
        {visited.includes('steps') && (
          <div className={frame('steps')} inert={workspace !== 'steps'}>
            {host === null ? (
              <Panel mark="ƒx" title="Transform · Claim timing" details={[status.text]}>
                {null}
              </Panel>
            ) : (
              <StepsWorkspace host={host} />
            )}
          </div>
        )}
        {visited.includes('table') && (
          <div className={frame('table')} inert={workspace !== 'table'}>
            {host === null ? (
              <Panel mark="▦" title="The table needs the core" details={[status.text]}>
                {null}
              </Panel>
            ) : (
              <TableViewer host={host} />
            )}
          </div>
        )}
        {visited.includes('pieces') && (
          <div className={frame('pieces')} inert={workspace !== 'pieces'}>
            {/* The pieces are plain HTML and CSS, tried out apart from the features. */}
            {/* oxlint-disable-next-line react/iframe-missing-sandbox -- the frame shows a page of this application, from its own origin */}
            <iframe
              title="Pieces"
              src="pieces/index.html"
              className="h-full flex-1 rounded-xl border border-line bg-panel"
            />
          </div>
        )}
      </main>
    </div>
  );
}
