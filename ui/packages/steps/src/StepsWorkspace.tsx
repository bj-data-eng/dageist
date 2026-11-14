/**
 * @file The main window's view of a transform: the table in the centre and
 * the steps down the right, each said by its head. Choosing a step shows the
 * rows after it, and a step is dragged to another place in the list. The
 * steps are edited in a window of their own, which a button opens. What is
 * changed there shows here at once.
 */

import { useState } from 'react';

import { Panel, cx } from '@dageist/design';
import type { Host } from '@dageist/host';

import { moved } from './Chain';
import { Pipeline } from './Pipeline';
import { RowsPreview } from './RowsPreview';
import { StepNavigator } from './StepNavigator';
import { SOURCE_FRAME } from './model/frame';
import { runSteps } from './model/steps';
import { useSharedSteps } from './shared';

/** What the centre shows: the rows after the chosen step, or the Polars code of every step. */
type Centre = 'rows' | 'polars';

interface CentreTabProps {
  readonly active: boolean;
  readonly onChoose: () => void;
  readonly children: string;
}

function CentreTab({ active, onChoose, children }: CentreTabProps): React.JSX.Element {
  return (
    <button
      type="button"
      aria-pressed={active}
      className={cx(
        '-mb-px cursor-pointer border-b-2 px-1 py-2 text-xs font-semibold tracking-wide',
        active ? 'border-accent text-ink' : 'border-transparent text-muted hover:text-ink',
      )}
      onClick={onChoose}
    >
      {children}
    </button>
  );
}

/** What the steps workspace needs from the shell. */
export interface StepsWorkspaceProps {
  /** Opens the steps window and carries the steps between the windows. */
  readonly host: Host;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
}

/** Draws the table of the sample claims with its steps listed beside it. */
export function StepsWorkspace({ host, styleNonce }: StepsWorkspaceProps): React.JSX.Element {
  const { steps, selectedId, stepsOpen, change } = useSharedSteps(host, 'main');
  const [centre, setCentre] = useState<Centre>('rows');
  const results = runSteps(steps);
  const selectedIndex = steps.findIndex((step) => step.id === selectedId);
  const shown = results[selectedIndex];
  const openSteps = (): void => {
    // A window that cannot be opened leaves the table as it was; there is nothing to undo.
    void host.openWindow('steps').catch(() => undefined);
  };

  return (
    <Panel
      mark="ƒx"
      title="Transform · Claim timing"
      details={[
        'The rows after the chosen step. Choose a step on the right to see how the table changes, and drag a step to another place. Open the editor to change the steps.',
      ]}
    >
      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          <div className="flex gap-4 border-b border-line px-4">
            <CentreTab
              active={centre === 'rows'}
              onChoose={() => {
                setCentre('rows');
              }}
            >
              {shown === undefined
                ? 'Rows as they arrive'
                : `Rows after step ${String(selectedIndex + 1)}`}
            </CentreTab>
            <CentreTab
              active={centre === 'polars'}
              onChoose={() => {
                setCentre('polars');
              }}
            >
              Polars
            </CentreTab>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
            {centre === 'rows' ? (
              <RowsPreview frame={shown?.frame ?? SOURCE_FRAME} added={shown?.added ?? []} />
            ) : (
              <Pipeline
                source="claims"
                steps={steps}
                selectedId={selectedId}
                onSelect={(id) => {
                  change(steps, id);
                }}
              />
            )}
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
              {stepsOpen ? 'Go to the editor' : 'Open the editor'}
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <StepNavigator
              steps={steps}
              results={results}
              selectedId={selectedId}
              styleNonce={styleNonce}
              onSelect={(id) => {
                // Choosing a step only shows its rows. The editor is asked for on its own.
                change(steps, id);
              }}
              onMove={(from, to) => {
                change(moved(steps, from, to), selectedId);
              }}
              onOpen={(id) => {
                change(steps, id);
                openSteps();
              }}
            />
          </div>
          {stepsOpen && (
            <p className="m-0 border-t border-line px-3 py-2 text-xs text-muted">
              The editor is open in its own window. What is changed there shows here.
            </p>
          )}
        </aside>
      </div>
    </Panel>
  );
}
