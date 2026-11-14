/**
 * @file The steps as an ordered list of rows that can be edited. Each step
 * works on the rows of the step before it. A step is dragged by its number
 * to another place, and a faint step waits after the last one.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { useMemo, useState } from 'react';

import { Slot } from '@dageist/design';

import type { DragSettings } from './Chain';
import { DragSettingsContext } from './Chain';
import type { Fresh } from './Lists';
import { FreshContext } from './Lists';
import { STEP_GROUP, StepRow, verbOffers } from './StepRow';
import type { Frame } from './model/frame';
import { SOURCE_FRAME } from './model/frame';
import type { Step, StepResult, Verb } from './model/steps';
import { defaultStep } from './model/steps';

function moved<T>(items: readonly T[], from: number, to: number): readonly T[] {
  const next = [...items];
  const [item] = next.splice(from, 1);
  if (item === undefined) return items;
  next.splice(to, 0, item);
  return next;
}

/** What a step list shows and reports. */
export interface StepListProps {
  readonly steps: readonly Step[];
  /** What each step produced, in the same order. */
  readonly results: readonly StepResult[];
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
  // What was just added, so that its first slot opens or its name is typed straight away.
  const [freshId, setFreshId] = useState<string | null>(null);
  const fresh = useMemo((): Fresh => ({ id: freshId, mark: setFreshId }), [freshId]);

  // Dragging injects a few style rules; under a content security policy they
  // need the page's nonce. Without a policy dnd kit's defaults are right.
  const dragSettings = useMemo(
    (): DragSettings =>
      styleNonce === null
        ? {}
        : {
            plugins: (defaults: Plugins): Plugins => [
              ...defaults,
              StyleInjector.configure({ nonce: styleNonce }),
            ],
          },
    [styleNonce],
  );

  const frameBefore = (index: number): Frame => results[index - 1]?.frame ?? SOURCE_FRAME;
  const addStep = (verb: Verb, after: number): void => {
    const added = defaultStep(verb, results[after]?.frame ?? SOURCE_FRAME);
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
                  <StepRow
                    key={step.id}
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
                    onInsertAfter={(verb) => {
                      addStep(verb, index);
                    }}
                  />
                );
              })}
            </div>
          </DragDropProvider>
          {/* A faint step waits after the last one. */}
          <div className="dg-ground mt-2 pl-[42px]">
            <Slot
              label="step"
              tone="ink"
              look="plain"
              waits
              describedAs="Add a step at the end"
              placeholder="Search the steps"
              offers={(typed) =>
                verbOffers(typed, (verb) => {
                  addStep(verb, steps.length - 1);
                })
              }
            />
          </div>
        </div>
      </FreshContext.Provider>
    </DragSettingsContext.Provider>
  );
}
