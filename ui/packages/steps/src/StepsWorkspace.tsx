/**
 * @file The main window's view of a transform: the table in the centre and
 * the steps down the right, each said by its head. Choosing a step shows the
 * rows after it, and a step is dragged to another place in the list. The
 * steps are edited in a window of their own, which a button opens. What is
 * changed there shows here at once.
 */

import { Panel } from '@dageist/design';
import type { Host } from '@dageist/host';

import { DraftsProvider } from './drafts';
import { EditorProvider, useListEdits } from './editor';
import { EngineProvider } from './EngineProvider';
import { RowsPreview } from './RowsPreview';
import { StepNavigator } from './StepNavigator';
import { useEditorWindow } from './window';

/** What the steps workspace needs from the shell. */
export interface StepsWorkspaceProps {
  /** Opens the steps window and carries the chosen step between the windows. */
  readonly host: Host;
  /** The id of the transform whose steps are edited. */
  readonly node: string;
  /** The transform's name, which the panel's title shows. */
  readonly name: string;
  /** The graph's revision, which the editor asks again with when it changes. */
  readonly graph: number;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
}

/**
 * Draws the rows the transform's steps start from with its steps listed beside them, once the
 * catalogue is there. Another transform is drawn afresh, with nothing carried over.
 */
export function StepsWorkspace({
  host,
  node,
  name,
  graph,
  styleNonce,
}: StepsWorkspaceProps): React.JSX.Element {
  return (
    <EngineProvider key={node} host={host} node={node} graph={graph} title={`Transform · ${name}`}>
      <Transform host={host} node={node} name={name} graph={graph} styleNonce={styleNonce} />
    </EngineProvider>
  );
}

function Transform({
  host,
  node,
  name,
  graph,
  styleNonce,
}: StepsWorkspaceProps): React.JSX.Element {
  const window = useEditorWindow(host, node, graph, 'main');
  const { editor, drafts, selection, steps, run, selectedIndex, rows } = window;
  return (
    <EditorProvider value={editor}>
      <DraftsProvider value={drafts}>
        <Body
          host={host}
          node={node}
          name={name}
          graph={graph}
          styleNonce={styleNonce}
          window={window}
          steps={steps}
          selectedIndex={selectedIndex}
          rows={rows}
          run={run}
          selection={selection}
        />
      </DraftsProvider>
    </EditorProvider>
  );
}

interface BodyProps extends StepsWorkspaceProps {
  readonly window: ReturnType<typeof useEditorWindow>;
  readonly steps: ReturnType<typeof useEditorWindow>['steps'];
  readonly selectedIndex: number;
  readonly rows: ReturnType<typeof useEditorWindow>['rows'];
  readonly run: ReturnType<typeof useEditorWindow>['run'];
  readonly selection: ReturnType<typeof useEditorWindow>['selection'];
}

function Body({
  host,
  name,
  styleNonce,
  window,
  steps,
  selectedIndex,
  rows,
  run,
  selection,
}: BodyProps): React.JSX.Element {
  const listEdits = useListEdits();
  const { editor } = window;
  const results = run?.steps ?? [];
  const shown = results[selectedIndex];
  const openSteps = (): void => {
    // A window that cannot be opened leaves the table as it was; there is nothing to undo.
    void host.openWindow('steps').catch(() => undefined);
  };

  return (
    <Panel
      mark="ƒx"
      title={`Transform · ${name}`}
      details={[
        'The rows after the chosen step. Choose a step on the right to see how the table changes, and drag a step to another place. Open the editor to change the steps.',
      ]}
    >
      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          <div className="flex gap-4 border-b border-line px-4">
            <h2 className="m-0 border-b-2 border-accent px-1 py-2 text-xs font-semibold tracking-wide text-ink">
              {shown === undefined
                ? 'Rows as they arrive'
                : `Rows after step ${String(selectedIndex + 1)}`}
            </h2>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
            <RowsPreview
              display={rows.display}
              changes={shown?.changes ?? []}
              settled={rows.settled}
            />
          </div>
        </section>
        <aside className="flex w-[340px] shrink-0 flex-col border-l border-line bg-raised">
          <div className="flex items-baseline gap-2 border-b border-line px-3 py-2">
            <h2 className="m-0 text-xs font-semibold tracking-wide text-muted">Steps</h2>
            <span className="font-mono text-[11px] text-muted">{steps.length}</span>
            <button
              type="button"
              title="Opens every step as its blocks, in a window of its own. A double click on a step does the same."
              className="ml-auto cursor-pointer rounded-md border border-line-strong px-2 py-0.5 text-xs font-semibold text-ink hover:border-accent hover:bg-hover"
              onClick={openSteps}
            >
              {selection.stepsOpen ? 'Go to the editor' : 'Open the editor'}
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <StepNavigator
              steps={steps}
              results={results}
              selectedId={selection.selectedId}
              styleNonce={styleNonce}
              onSelect={(id) => {
                // Choosing a step only shows its rows. The editor is asked for on its own.
                selection.choose(id);
              }}
              onMove={(from, to) => {
                const moved = steps[from];
                if (moved !== undefined) void listEdits.moveStep({ index: from, id: moved.id }, to);
              }}
              onOpen={(id) => {
                selection.choose(id);
                openSteps();
              }}
            />
          </div>
          {editor.failure !== null && (
            <p className="m-0 border-t border-line px-3 py-2 text-xs text-wrong">
              {editor.failure}
            </p>
          )}
          {editor.notice !== null && (
            <p className="m-0 border-t border-line px-3 py-2 text-xs text-wrong">
              {editor.notice}{' '}
              <button type="button" className="cursor-pointer underline" onClick={editor.dismiss}>
                Dismiss
              </button>
            </p>
          )}
          {selection.stepsOpen && (
            <p className="m-0 border-t border-line px-3 py-2 text-xs text-muted">
              The editor is open in its own window. What is changed there shows here.
            </p>
          )}
        </aside>
      </div>
    </Panel>
  );
}
