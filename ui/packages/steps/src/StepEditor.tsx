/**
 * @file The step editor: a transformation built as an ordered list of steps.
 * Each step works on the rows of the step before it. The list is kept dense:
 * one row per step, with tools that appear where the pointer is. Below it a
 * dock shows either the rows after the chosen step or the Polars code the
 * list stands for.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { useMemo, useState } from 'react';

import { Panel, Picker, cx } from '@dageist/design';

import type { DragSettings } from './ChainEditor';
import { DragSettingsContext } from './ChainEditor';
import { InputsStrip } from './InputsStrip';
import { Pipeline } from './Pipeline';
import { RowsPreview } from './RowsPreview';
import { STEP_GROUP, StepCard, verbChoices } from './StepCard';
import type { Frame } from './model/frame';
import { SOURCE_FRAME } from './model/frame';
import type { Step } from './model/steps';
import { defaultStep, runSteps } from './model/steps';
import { START } from './start';

/** What the dock under the list shows. */
type Dock = 'rows' | 'polars';

function moved<T>(items: readonly T[], from: number, to: number): readonly T[] {
  const next = [...items];
  const [item] = next.splice(from, 1);
  if (item === undefined) return items;
  next.splice(to, 0, item);
  return next;
}

interface DockTabProps {
  readonly active: boolean;
  readonly onChoose: () => void;
  readonly children: string;
}

function DockTab({ active, onChoose, children }: DockTabProps): React.JSX.Element {
  return (
    <button
      type="button"
      aria-pressed={active}
      className={cx(
        '-mb-px cursor-pointer border-b-2 px-1 py-1.5 text-xs font-semibold tracking-wide',
        active ? 'border-accent text-ink' : 'border-transparent text-muted hover:text-ink',
      )}
      onClick={onChoose}
    >
      {children}
    </button>
  );
}

/** What the step editor needs from the shell. */
export interface StepEditorProps {
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
}

/** Draws the step editor on the sample claims. */
export function StepEditor({ styleNonce }: StepEditorProps): React.JSX.Element {
  const [steps, setSteps] = useState<readonly Step[]>(START);
  const [selectedId, setSelectedId] = useState<string | null>(START[2]?.id ?? null);
  const [dock, setDock] = useState<Dock>('rows');

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

  const results = runSteps(steps);
  const frameBefore = (index: number): Frame => results[index - 1]?.frame ?? SOURCE_FRAME;
  const selectedIndex = steps.findIndex((step) => step.id === selectedId);
  const shown = results[selectedIndex];
  const last = results.at(-1)?.frame ?? SOURCE_FRAME;

  const replaceStep = (next: Step): void => {
    setSteps(steps.map((step) => (step.id === next.id ? next : step)));
  };

  return (
    <DragSettingsContext.Provider value={dragSettings}>
      <Panel
        mark="ƒx"
        title="Transform · Claim timing"
        details={[
          'Point at a step or a part for its tools and what it returns. A › between parts becomes a + to insert a method.',
        ]}
      >
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
            <InputsStrip />
            <DragDropProvider
              {...dragSettings}
              onDragEnd={(event) => {
                const { source } = event.operation;
                if (event.canceled || !isSortable(source)) return;
                if (source.group === STEP_GROUP && source.index !== source.initialIndex) {
                  setSteps(moved(steps, source.initialIndex, source.index));
                }
              }}
            >
              <div className="rounded-lg border border-line">
                {steps.map((step, index) => {
                  const result = results[index];
                  if (result === undefined) return null;
                  return (
                    <StepCard
                      key={step.id}
                      step={step}
                      index={index}
                      frame={frameBefore(index)}
                      result={result}
                      selected={step.id === selectedId}
                      onSelect={() => {
                        setSelectedId(step.id);
                      }}
                      onChange={replaceStep}
                      onRemove={() => {
                        setSteps(steps.filter((existing) => existing.id !== step.id));
                        if (step.id === selectedId) {
                          // The rows shown move to the step before, or the one after the first.
                          setSelectedId((steps[index - 1] ?? steps[index + 1])?.id ?? null);
                        }
                      }}
                      onInsertAfter={(verb) => {
                        const added = defaultStep(verb, result.frame);
                        setSteps([...steps.slice(0, index + 1), added, ...steps.slice(index + 1)]);
                        setSelectedId(added.id);
                      }}
                    />
                  );
                })}
              </div>
            </DragDropProvider>
            <div className="mt-1.5 pl-8">
              <Picker
                label="+ Add step"
                tone="plain"
                className="text-muted hover:text-ink"
                choices={verbChoices(null, (verb) => {
                  const added = defaultStep(verb, last);
                  setSteps([...steps, added]);
                  setSelectedId(added.id);
                })}
              />
            </div>
          </div>
          <div className="flex max-h-[48%] min-h-0 shrink-0 flex-col border-t border-line bg-raised">
            <div className="flex gap-4 border-b border-line px-4">
              <DockTab
                active={dock === 'rows'}
                onChoose={() => {
                  setDock('rows');
                }}
              >
                {shown === undefined
                  ? 'Rows as they arrive'
                  : `Rows after step ${String(selectedIndex + 1)}`}
              </DockTab>
              <DockTab
                active={dock === 'polars'}
                onChoose={() => {
                  setDock('polars');
                }}
              >
                Polars
              </DockTab>
            </div>
            <div className="min-h-0 overflow-auto px-4 py-2">
              {dock === 'rows' ? (
                <RowsPreview frame={shown?.frame ?? SOURCE_FRAME} added={shown?.added ?? []} />
              ) : (
                <Pipeline
                  source="claims"
                  steps={steps}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                />
              )}
            </div>
          </div>
        </div>
      </Panel>
    </DragSettingsContext.Provider>
  );
}
