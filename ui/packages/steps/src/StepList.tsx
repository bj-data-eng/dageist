/**
 * @file The steps as an ordered list of rows that can be edited. Each step
 * works on the rows of the step before it. A step is dragged by its number
 * to another place, and a faint step waits after the last one.
 */

import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { useMemo, useState } from 'react';

import type { StepStartView } from '@dageist/host';
import { Slot } from '@dageist/design';

import { useCatalogue, useNode } from './EngineProvider';
import { PreparedStepProvider } from './engine';
import { useStepStarts } from './ask';
import { DragSettingsContext, dragSettingsFor, moved } from './Chain';
import type { Fresh } from './Lists';
import { FreshContext } from './Lists';
import { useListState } from './positions';
import { STEP_GROUP, StepRow, verbOffers } from './StepRow';
import type { Frame } from './model/frame';
import type { PreparedStep } from './model/report';
import { startedStep } from './model/started';
import type { Step, Verb } from './model/steps';
import type { Origins } from './origins';
import { OriginsContext, originsBefore } from './origins';

/** What a step with no results before it has: no field made by a step. */
const NO_ORIGINS: Origins = new Map();

/** What a step list shows and reports. */
export interface StepListProps {
  readonly steps: readonly Step[];
  /** What each step produced, in the same order. */
  readonly results: readonly PreparedStep[];
  readonly selectedId: string | null;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
  /** Reports the steps and the chosen step after a change to either. */
  readonly onChange: (steps: readonly Step[], selectedId: string | null) => void;
}

/** Draws `steps` as rows that can be edited, chosen, dragged, added to and removed. */
export function StepList({
  steps,
  results,
  selectedId,
  styleNonce,
  onChange,
}: StepListProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const node = useNode();
  // What was just added, so that its first slot opens or its name is typed straight away.
  const [freshId, setFreshId] = useState<string | null>(null);
  const fresh = useMemo((): Fresh => ({ id: freshId, mark: setFreshId }), [freshId]);

  const dragSettings = useMemo(() => dragSettingsFor(styleNonce), [styleNonce]);

  const frameBefore = (index: number): Frame => results[index]?.before ?? node.source.columns;
  // Where each step's fields come from, for the hints that say so.
  const origins = originsBefore(results);
  // What each step starts with at the end of the list, asked of the engine while its list is open.
  const endList = useListState();
  const endStarts = useStepStarts(endList.open, results.at(-1)?.after ?? node.source.columns, node);
  const addStep = (verb: Verb, start: StepStartView, after: number): void => {
    const added = startedStep(verb, start, catalogue.comparisons);
    onChange([...steps.slice(0, after + 1), added, ...steps.slice(after + 1)], added.id);
    // What is gone to first: the field of a new filter's comparison, or the step itself.
    setFreshId(added.verb === 'filter' ? added.condition.id : added.id);
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
              const { source } = event.operation;
              if (event.canceled || !isSortable(source)) return;
              if (source.group === STEP_GROUP && source.index !== source.initialIndex) {
                onChange(moved(steps, source.initialIndex, source.index), selectedId);
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
                          if (step.id !== selectedId) onChange(steps, step.id);
                        }}
                        onChange={(next) => {
                          onChange(
                            steps.map((existing) => (existing.id === next.id ? next : existing)),
                            selectedId,
                          );
                        }}
                        onRemove={() => {
                          // The rows shown move to the step before, or the one after the first.
                          const beside = (steps[index - 1] ?? steps[index + 1])?.id ?? null;
                          onChange(
                            steps.filter((existing) => existing.id !== step.id),
                            step.id === selectedId ? beside : selectedId,
                          );
                        }}
                        onInsertAfter={(verb, start) => {
                          addStep(verb, start, index);
                        }}
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
                verbOffers(catalogue, typed, endStarts, (verb, start) => {
                  addStep(verb, start, steps.length - 1);
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
