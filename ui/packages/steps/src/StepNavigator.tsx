/**
 * @file The steps as a list of names: each step said by its head, such as
 * "Sort table by dcn" or "Add column step_index", without the pieces under
 * it. A click chooses a step, so that its rows can be looked at. A step is
 * dragged to another place in the list, and a double click asks for the
 * editor.
 */

import { PointerActivationConstraints, PointerSensor } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable, useSortable } from '@dnd-kit/react/sortable';
import { useMemo } from 'react';

import { cx } from '@dageist/design';

import { dragSettingsFor } from './Chain';
import type { Step, StepResult } from './model/steps';
import { stepDetail } from './summary';
import { VERB_COLOURS, VERB_LABELS } from './verbs';

/** The sortable group the names of the steps belong to. */
const NAME_GROUP = 'step-names';

/**
 * A step's name is dragged by the same button that chooses it. Dragging
 * starts only after the pointer has moved, so a plain click still chooses.
 */
const NAME_SENSORS = [
  PointerSensor.configure({
    activationConstraints: [new PointerActivationConstraints.Distance({ value: 6 })],
    preventActivation: () => false,
  }),
];

interface StepNameProps {
  readonly step: Step;
  readonly index: number;
  /** Why the step cannot be worked out, or `null` when it can. */
  readonly problem: string | null;
  readonly selected: boolean;
  readonly onSelect: () => void;
  readonly onOpen: (() => void) | null;
}

/** Draws one step's name as a button that chooses it and a thing that can be dragged. */
function StepName({
  step,
  index,
  problem,
  selected,
  onSelect,
  onOpen,
}: StepNameProps): React.JSX.Element {
  const { ref, isDragSource } = useSortable({
    id: step.id,
    index,
    group: NAME_GROUP,
    type: 'step-name',
    accept: 'step-name',
    sensors: NAME_SENSORS,
  });
  return (
    <li ref={ref} className={cx(isDragSource && 'opacity-60')}>
      <button
        type="button"
        aria-current={selected ? 'step' : undefined}
        title={problem ?? undefined}
        className={cx(
          'flex w-full cursor-pointer touch-none items-baseline gap-2 border-b border-line bg-raised px-3 py-1.5 text-left text-[13px] select-none',
          'hover:bg-hover focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent',
          VERB_COLOURS[step.verb],
          selected && 'bg-row shadow-[inset_2px_0_0_var(--color-accent)]',
          problem !== null && !selected && 'shadow-[inset_2px_0_0_var(--color-wrong)]',
        )}
        onClick={onSelect}
        onDoubleClick={onOpen ?? undefined}
      >
        <span className="w-5 shrink-0 text-right font-mono text-[11px] text-muted">
          {index + 1}
        </span>
        <span className="shrink-0 font-semibold text-(--verb)">{VERB_LABELS[step.verb]}</span>
        <span
          className={cx(
            'min-w-0 truncate font-mono text-[12.5px]',
            problem === null ? 'text-ink' : 'text-wrong',
          )}
        >
          {stepDetail(step)}
        </span>
      </button>
    </li>
  );
}

/** What a step navigator shows and reports. */
export interface StepNavigatorProps {
  readonly steps: readonly Step[];
  /** What each step produced, in the same order. */
  readonly results: readonly StepResult[];
  readonly selectedId: string | null;
  /**
   * The nonce the page's content security policy accepts for injected styles,
   * or `null` when there is no policy. Dragging injects a few style rules.
   */
  readonly styleNonce: string | null;
  readonly onSelect: (id: string) => void;
  /** Reports that the step at `from` was dragged to the place `to`. */
  readonly onMove: (from: number, to: number) => void;
  /** Asks for the editor on a step that was double-clicked, or `null` where the editor is here. */
  readonly onOpen: ((id: string) => void) | null;
}

/** Lists `steps` by name, and reports the one that is chosen and where one is dragged to. */
export function StepNavigator({
  steps,
  results,
  selectedId,
  styleNonce,
  onSelect,
  onMove,
  onOpen,
}: StepNavigatorProps): React.JSX.Element {
  const settings = useMemo(() => dragSettingsFor(styleNonce), [styleNonce]);
  return (
    <DragDropProvider
      {...settings}
      onDragEnd={(event) => {
        const { source } = event.operation;
        if (event.canceled || !isSortable(source)) return;
        if (source.index !== source.initialIndex) onMove(source.initialIndex, source.index);
      }}
    >
      <ol className="m-0 flex list-none flex-col p-0">
        {steps.map((step, index) => (
          <StepName
            key={step.id}
            step={step}
            index={index}
            problem={results[index]?.problem ?? null}
            selected={step.id === selectedId}
            onSelect={() => {
              onSelect(step.id);
            }}
            onOpen={
              onOpen === null
                ? null
                : () => {
                    onOpen(step.id);
                  }
            }
          />
        ))}
      </ol>
    </DragDropProvider>
  );
}
