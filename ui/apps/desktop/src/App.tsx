/**
 * @file The shell: the composition root that decides which host the view
 * talks to and which feature is on screen. Features never import each other;
 * they meet only here.
 */

import { useEffect, useState } from 'react';

import { FlowCanvas } from '@dageist/canvas';
import { Panel, SidebarProvider, SidebarTrigger, cx } from '@dageist/design';
import type { Host } from '@dageist/host';
import { bridgeHost, tauriHost } from '@dageist/host';
import { StepsWindow, StepsWorkspace } from '@dageist/steps';
import { TableViewer } from '@dageist/table';

import { behaveLikeAnApplication } from './native-feel';
import { AppSidebar, CONTROLS_ROOM, TOP_ROW } from './Sidebar';

/** A feature the shell can put on screen. */
type Workspace = 'steps' | 'flow' | 'table' | 'pieces';

/** A workspace, what the side panel calls it and the glyph beside its name. */
interface WorkspaceTab {
  readonly id: Workspace;
  readonly label: string;
  readonly mark: string;
}

const WORKSPACES: readonly WorkspaceTab[] = [
  { id: 'flow', label: 'Flow', mark: '⌗' },
  { id: 'steps', label: 'Transform', mark: 'ƒx' },
  { id: 'table', label: 'Table', mark: '▦' },
  { id: 'pieces', label: 'Pieces', mark: '◇' },
];

/**
 * Returns the workspace the address names after its `#`, or the flow.
 * The address keeps the workspace, so reloading the page stays on it.
 */
function startingWorkspace(): Workspace {
  const named = window.location.hash.slice(1);
  return WORKSPACES.find((entry) => entry.id === named)?.id ?? 'flow';
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

/** Returns the host to use: Tauri when it is there, the local bridge in a plain browser. */
function findHost(): Host {
  return tauriHost() ?? bridgeHost();
}

const STYLE_NONCE = readStyleNonce();

/**
 * Whether this page is the steps window and not the main one. The address
 * says so after its `?`, which is how the host opens that window.
 */
const IS_STEPS_WINDOW = new URLSearchParams(window.location.search).get('window') === 'steps';

/**
 * Whether the page is drawn in the application's own window, which has the
 * Tauri host, and not in a browser, which reaches the host through the
 * bridge. The window draws its own controls at the top left; a browser does not.
 */
const IN_APPLICATION_WINDOW = tauriHost() !== null;

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

  // In the application's window the browser's own menu and keys are off. In a
  // browser they stay, for the preview is a page being worked on.
  useEffect(() => (IN_APPLICATION_WINDOW ? behaveLikeAnApplication() : undefined), []);

  useEffect(() => {
    let live = true;
    const connect = async (): Promise<void> => {
      try {
        const found = findHost();
        // The host opens windows and carries messages whether or not the core answers.
        if (live) setHost(found);
        const identity = await found.coreIdentity();
        if (!live) return;
        setStatus({ text: `${identity.name} ${identity.version}`, tone: 'ready' });
      } catch (error) {
        if (live) {
          const reason = error instanceof Error ? error.message : String(error);
          setStatus({ text: `Core unavailable: ${reason}`, tone: 'failed' });
        }
      }
    };
    void connect();
    return () => {
      live = false;
    };
  }, []);

  const [sidebarOpen, setSidebarOpen] = useState(true);

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

  const statusLine = (
    <output className="flex items-center gap-2">
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
  );

  // The side panel stands at the left, and the flow canvas fills the rest of
  // the window as its background. Every other workspace is drawn over the
  // canvas. With the panel hidden, the window's controls, the toggle and the
  // workspace's name float over the canvas's top left corner.
  return (
    <SidebarProvider
      open={sidebarOpen}
      onOpenChange={setSidebarOpen}
      className="h-full min-h-0 overflow-hidden bg-canvas"
    >
      <AppSidebar workspaces={WORKSPACES} current={workspace} onShow={show} status={statusLine} />
      {/*
       * The toggle has one place, fixed beside the window's controls and drawn
       * above the side panel, so it stays still while the panel slides in or
       * out beneath it. The workspaces are chosen in the panel only.
       */}
      <div
        data-tauri-drag-region
        className={cx(
          'fixed top-0 left-0 z-20 flex items-center gap-1',
          TOP_ROW,
          IN_APPLICATION_WINDOW ? CONTROLS_ROOM : 'pl-4',
        )}
      >
        {/* A browser draws no window controls here, so the application's name takes their place. */}
        {!IN_APPLICATION_WINDOW && (
          <span className="mr-2 text-[14px] font-semibold text-ink">dageist</span>
        )}
        <SidebarTrigger className="text-muted" />
      </div>
      <div className="relative min-w-0 flex-1 bg-dots">
        <div className="absolute inset-0">{host !== null && <FlowCanvas host={host} />}</div>
        <main
          className={cx(
            'absolute inset-x-0 top-[52px] bottom-0 flex min-h-0 flex-col px-7 pb-6',
            workspace === 'flow' && 'pointer-events-none',
          )}
        >
          {visited.includes('steps') && (
            <div className={frame('steps')} inert={workspace !== 'steps'}>
              {host === null ? (
                <Panel mark="ƒx" title="Transform · Claim timing" details={[status.text]}>
                  {null}
                </Panel>
              ) : (
                <StepsWorkspace host={host} styleNonce={STYLE_NONCE} />
              )}
            </div>
          )}
          {visited.includes('table') && (
            <div className={frame('table')} inert={workspace !== 'table'}>
              {host === null || status.tone !== 'ready' ? (
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
    </SidebarProvider>
  );
}
