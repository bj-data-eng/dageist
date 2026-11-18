/**
 * @file The steps window: every step of the transform as the blocks it is
 * written with, beside a list of the steps by name. The step that is chosen
 * here is the one whose rows the main window shows, and a change made here
 * shows there at once.
 */

import { useEffect } from 'react';

import { Panel } from '@dageist/design';
import type { Host } from '@dageist/host';

import { EngineProvider, useCatalogue } from './EngineProvider';
import { moved } from './Chain';
import { useRun } from './engine';
import { InputsStrip } from './InputsStrip';
import { StepList } from './StepList';
import { StepNavigator } from './StepNavigator';
import { useSharedSteps } from './shared';

/** What the steps window needs from the shell. */
export interface StepsWindowProps {
  /** Carries the steps between this window and the main one. */
  readonly host: Host;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
}

/** Draws every step of the sample claims so that it can be edited, once the catalogue is there. */
export function StepsWindow({ host, styleNonce }: StepsWindowProps): React.JSX.Element {
  return (
    <EngineProvider host={host} title="Steps · Claim timing">
      <Editor host={host} styleNonce={styleNonce} />
    </EngineProvider>
  );
}

function Editor({ host, styleNonce }: StepsWindowProps): React.JSX.Element {
  const { steps, selectedId, change } = useSharedSteps(host, 'steps', useCatalogue().comparisons);
  const run = useRun(host, steps);
  const results = run.prepared.steps;

  // The chosen step is brought into view, whichever window chose it.
  useEffect(() => {
    if (selectedId === null) return;
    document
      .querySelector(`[data-step-id="${CSS.escape(selectedId)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [selectedId]);

  return (
    <Panel
      mark="ƒx"
      title="Steps · Claim timing"
      details={[
        'Click a word to change it. The list it opens also adds after the thing the word names, or removes it. Drag a step by its number.',
      ]}
    >
      <div className="flex min-h-0 flex-1">
        <nav
          aria-label="Steps"
          className="w-[280px] shrink-0 overflow-y-auto border-r border-line bg-raised"
        >
          <StepNavigator
            steps={steps}
            results={results}
            selectedId={selectedId}
            styleNonce={styleNonce}
            onSelect={(id) => {
              change(steps, id);
            }}
            onMove={(from, to) => {
              change(moved(steps, from, to), selectedId);
            }}
            onOpen={null}
          />
        </nav>
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto px-4 py-3">
          <InputsStrip />
          {run.failure !== null && <p className="m-0 mb-3 text-xs text-wrong">{run.failure}</p>}
          <StepList
            steps={steps}
            results={results}
            selectedId={selectedId}
            styleNonce={styleNonce}
            onChange={change}
          />
        </div>
      </div>
    </Panel>
  );
}
