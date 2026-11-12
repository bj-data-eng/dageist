/**
 * @file The step editor: a transformation built as an ordered list of steps.
 * Each step works on the rows of the step before it. The editor shows the
 * list, the Polars code it stands for, the rows after the chosen step and
 * the methods that can be added to it.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { useMemo, useState } from 'react';

import { Panel, Picker } from '@dageist/design';

import { withMethodMoved } from './ExprChain';
import { MethodPalette } from './MethodPalette';
import { Pipeline } from './Pipeline';
import { RowsPreview } from './RowsPreview';
import { STEP_GROUP, StepCard, verbChoices } from './StepCard';
import type { Method } from './model/expr';
import type { Frame } from './model/frame';
import { SOURCE_FRAME } from './model/frame';
import type { Step } from './model/steps';
import { defaultStep, runSteps } from './model/steps';
import { START } from './start';

function moved<T>(items: readonly T[], from: number, to: number): readonly T[] {
  const next = [...items];
  const [item] = next.splice(from, 1);
  if (item === undefined) return items;
  next.splice(to, 0, item);
  return next;
}

function SectionTitle({ children }: { readonly children: string }): React.JSX.Element {
  return <h2 className="mt-4 mb-2 text-xs font-semibold tracking-wide text-muted">{children}</h2>;
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

  // Dragging injects a few style rules; under a content security policy they
  // need the page's nonce. Without a policy dnd kit's defaults are right.
  const dragOptions = useMemo(
    () =>
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
  const selected = steps[selectedIndex] ?? null;
  const shown = results[selectedIndex];
  const last = results.at(-1)?.frame ?? SOURCE_FRAME;

  const replaceStep = (next: Step): void => {
    setSteps(steps.map((step) => (step.id === next.id ? next : step)));
  };

  /** Applies a finished drag: a step moved in the list, or a part moved in its chain. */
  const applyDrop = (group: string | number | undefined, from: number, to: number): void => {
    if (group === STEP_GROUP) {
      setSteps(moved(steps, from, to));
      return;
    }
    const owner = steps.find((step) => step.id === group);
    if (owner?.verb === 'with_columns') {
      replaceStep({ ...owner, expr: withMethodMoved(owner.expr, from, to) });
    }
  };

  return (
    <Panel
      mark="ƒx"
      title="Transform · Claim timing"
      details={[
        'Each step works on the rows of the step before it. Drag a step or a part to move it.',
      ]}
    >
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_320px]">
        <div className="overflow-y-auto px-5 pt-5 pb-6">
          <DragDropProvider
            {...dragOptions}
            onDragEnd={(event) => {
              const { source } = event.operation;
              if (event.canceled || !isSortable(source)) return;
              if (source.index !== source.initialIndex) {
                applyDrop(source.group, source.initialIndex, source.index);
              }
            }}
          >
            <div className="flex flex-col gap-1.5">
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
                    }}
                  />
                );
              })}
            </div>
          </DragDropProvider>
          <div className="mt-2">
            <Picker
              label="+ Add step"
              tone="verb"
              choices={verbChoices(null, (verb) => {
                const added = defaultStep(verb, last);
                setSteps([...steps, added]);
                setSelectedId(added.id);
              })}
            />
          </div>
          <SectionTitle>Polars</SectionTitle>
          <Pipeline
            source="claims"
            steps={steps}
            selectedId={selectedId}
            onSelect={setSelectedId}
          />
          <SectionTitle>
            {shown === undefined
              ? 'Rows as they arrive'
              : `Rows after step ${String(selectedIndex + 1)}`}
          </SectionTitle>
          <RowsPreview frame={shown?.frame ?? SOURCE_FRAME} added={shown?.added ?? null} />
        </div>
        <MethodPalette
          step={selected}
          frame={frameBefore(selectedIndex)}
          onAdd={(method: Method) => {
            if (selected?.verb !== 'with_columns') return;
            const { expr } = selected;
            replaceStep({
              ...selected,
              expr: { column: expr.column, methods: [...expr.methods, method], alias: expr.alias },
            });
          }}
        />
      </div>
    </Panel>
  );
}
