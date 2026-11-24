/**
 * @file The steps as an ordered list of rows that can be edited. Each step works on the rows of the
 * step before it. A step is dragged by its number to another place, and a faint step waits after the
 * last one. What is drawn is the document the host holds, and each change is sent as an edit.
 */

import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { useMemo, useState } from 'react';

import type { DocumentStepView, FrameRequest } from '@dageist/host';
import { Slot } from '@dageist/design';

import { useStepStarts } from './ask';
import { DragSettingsContext, dragSettingsFor } from './Chain';
import { useListEdits } from './editor';
import { useCatalogue, useNode } from './EngineProvider';
import type { Fresh } from './Lists';
import { FreshContext } from './Lists';
import { useListState } from './positions';
import { freshKey } from './Parameters';
import { STEP_GROUP, StepRow, frameRequest, stepOffers } from './StepRow';
import { PreparedStepProvider } from './engine';
import type { Frame } from './model/frame';
import { parameterSegment, itemSegment } from './model/doc';
import { tablesOf } from './model/inputs';
import type { PreparedStep } from './model/report';
import type { Origins } from './origins';
import { OriginsContext, originsBefore } from './origins';

/** What a step with no results before it has: no field made by a step. */
const NO_ORIGINS: Origins = new Map();

/** What a step list shows and reports. */
export interface StepListProps {
  readonly steps: readonly DocumentStepView[];
  /** What each step produced, in the same order. */
  readonly results: readonly PreparedStep[];
  /** The columns of the table the steps start from. */
  readonly source: Frame;
  readonly selectedId: string | null;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
  /** Chooses the step called `id`. */
  readonly onChoose: (id: string | null) => void;
}

/** Draws `steps` as rows that can be edited, chosen, dragged, added to and removed. */
export function StepList({
  steps,
  results,
  source,
  selectedId,
  styleNonce,
  onChoose,
}: StepListProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const node = useNode();
  const listEdits = useListEdits();
  // What was just added, so that its first slot opens or its name is typed straight away.
  const [freshId, setFreshId] = useState<string | null>(null);
  const fresh = useMemo((): Fresh => ({ id: freshId, mark: setFreshId }), [freshId]);

  const dragSettings = useMemo(() => dragSettingsFor(styleNonce), [styleNonce]);

  const frameBefore = (index: number): Frame => results[index]?.before ?? source;
  // Where each step's fields come from, for the hints that say so.
  const origins = originsBefore(results);
  // What each step starts with at the end of the list, asked of the engine while its list is open.
  const endList = useListState();
  const endStarts = useStepStarts(endList.open, results.at(-1)?.after ?? source, node);
  const addStep = (at: number, entry: string, frame: FrameRequest): void => {
    void listEdits.addStep(at, entry, frame).then((sent) => {
      if (sent.status !== 'edited') return;
      const added = sent.state.document.steps[at];
      if (added === undefined) return;
      onChoose(added.id);
      // What is gone to first: the field of a new filter's comparison, or the step itself.
      setFreshId(
        entry === 'LazyFrame.filter'
          ? freshKey(added.id, [parameterSegment('predicates'), itemSegment(0)])
          : added.id,
      );
    });
  };
  // Whatever is done next, nothing is new any more.
  const settle = (): void => {
    if (freshId !== null) setFreshId(null);
  };

  return (
    <DragSettingsContext.Provider value={dragSettings}>
      <FreshContext.Provider value={fresh}>
        <div onPointerDownCapture={settle} onKeyDownCapture={settle}>
          <DragDropProvider
            {...dragSettings}
            onDragEnd={(event) => {
              const { source: dragged } = event.operation;
              if (event.canceled || !isSortable(dragged)) return;
              if (dragged.group === STEP_GROUP && dragged.index !== dragged.initialIndex) {
                const moved = steps[dragged.initialIndex];
                if (moved !== undefined) {
                  void listEdits.moveStep(
                    { index: dragged.initialIndex, id: moved.id },
                    dragged.index,
                  );
                }
              }
            }}
          >
            <div className="rounded-lg border border-line">
              {steps.map((step, index) => {
                const result = results[index];
                if (result === undefined) return null;
                return (
                  <OriginsContext.Provider key={step.id} value={origins[index] ?? NO_ORIGINS}>
                    <PreparedStepProvider value={result}>
                      <StepRow
                        step={step}
                        index={index}
                        frame={frameBefore(index)}
                        result={result}
                        selected={step.id === selectedId}
                        onSelect={() => {
                          if (step.id !== selectedId) onChoose(step.id);
                        }}
                        onAdd={addStep}
                      />
                    </PreparedStepProvider>
                  </OriginsContext.Provider>
                );
              })}
            </div>
          </DragDropProvider>
          {/* A faint step waits after the last one. */}
          <div className="dg-ground dg-code mt-1 pl-[44px] [text-indent:0]">
            <Slot
              label="step"
              tone="ink"
              look="plain"
              waits
              describedAs="Add a step at the end"
              placeholder="Search the steps"
              offers={(typed) =>
                stepOffers(catalogue, typed, endStarts, (entry) => {
                  addStep(
                    steps.length,
                    entry,
                    frameRequest(results.at(-1)?.after ?? source, tablesOf(node)),
                  );
                })
              }
              onOpenChange={endList.onOpenChange}
            />
          </div>
        </div>
      </FreshContext.Provider>
    </DragSettingsContext.Provider>
  );
}
