/**
 * @file The steps window: every step of the transform as the blocks it is
 * written with, beside a list of the steps by name. The step that is chosen
 * here is the one whose rows the main window shows, and a change made here
 * shows there at once.
 */

import { useEffect } from 'react';

import { Panel } from '@dageist/design';
import type { Host } from '@dageist/host';

import { DraftsProvider } from './drafts';
import { EditorProvider, useListEdits } from './editor';
import { EngineProvider } from './EngineProvider';
import { InputsStrip } from './InputsStrip';
import { StepList } from './StepList';
import { StepNavigator } from './StepNavigator';
import { useEditorWindow } from './window';

/** What the steps window needs from the shell. */
export interface StepsWindowProps {
  /** Carries the chosen step between this window and the main one. */
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
  const window = useEditorWindow(host, 'steps');
  return (
    <EditorProvider value={window.editor}>
      <DraftsProvider value={window.drafts}>
        <Body window={window} styleNonce={styleNonce} />
      </DraftsProvider>
    </EditorProvider>
  );
}

function Body({
  window,
  styleNonce,
}: {
  readonly window: ReturnType<typeof useEditorWindow>;
  readonly styleNonce: string | null;
}): React.JSX.Element {
  const { editor, selection, steps, run } = window;
  const listEdits = useListEdits();
  const results = run?.steps ?? [];
  const { selectedId } = selection;

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
            onSelect={selection.choose}
            onMove={(from, to) => {
              const moved = steps[from];
              if (moved !== undefined) void listEdits.moveStep({ index: from, id: moved.id }, to);
            }}
            onOpen={null}
          />
        </nav>
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto px-4 py-3">
          <InputsStrip />
          {editor.failure !== null && (
            <p className="m-0 mb-3 text-xs text-wrong">{editor.failure}</p>
          )}
          {editor.notice !== null && (
            <p className="m-0 mb-3 text-xs text-wrong">
              {editor.notice}{' '}
              <button type="button" className="cursor-pointer underline" onClick={editor.dismiss}>
                Dismiss
              </button>
            </p>
          )}
          {run !== null && (
            <StepList
              steps={steps}
              results={results}
              source={run.source}
              selectedId={selectedId}
              styleNonce={styleNonce}
              onChoose={selection.choose}
            />
          )}
        </div>
      </div>
    </Panel>
  );
}
