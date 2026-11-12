/**
 * @file One step of the list: its verb, its slots and what it returns. The
 * card is dragged by its grip to another place in the list.
 */

import { useSortable } from '@dnd-kit/react/sortable';

import type { Choice } from '@dageist/design';
import { CommitField, Picker, cx } from '@dageist/design';

import { ExprChain } from './ExprChain';
import { columnChoices } from './choices';
import { checkExpr } from './model/expr';
import type { Frame } from './model/frame';
import { distinctValues } from './model/frame';
import type { FilterStep, SortStep, Step, StepResult, Verb, WithColumnsStep } from './model/steps';
import { COMPARISONS, VERBS, defaultStep, literalText } from './model/steps';

/** The sortable group every step card belongs to. */
export const STEP_GROUP = 'steps';

/** What each step is called: Polars' own word where it has one, in a plain phrase. */
const VERB_LABELS: Readonly<Record<Verb, string>> = {
  with_columns: 'Add column',
  sort: 'Sort table',
  filter: 'Filter table',
};

/** Lists the verbs as menu choices. */
export function verbChoices(current: Verb | null, onPick: (verb: Verb) => void): Choice[] {
  return VERBS.map((verb) => ({
    label: VERB_LABELS[verb],
    current: verb === current,
    reason: null,
    onPick: () => {
      onPick(verb);
    },
  }));
}

function SlotLabel({ children }: { readonly children: string }): React.JSX.Element {
  return <span className="text-xs text-muted">{children}</span>;
}

interface SlotsProps<S extends Step> {
  readonly step: S;
  readonly frame: Frame;
  readonly onChange: (step: Step) => void;
}

function AddColumnSlots({ step, frame, onChange }: SlotsProps<WithColumnsStep>): React.JSX.Element {
  const { expr } = step;
  return (
    <>
      <CommitField
        value={expr.alias}
        label="New column name"
        numeric={false}
        code
        onCommit={(alias) => {
          onChange({ ...step, expr: { column: expr.column, methods: expr.methods, alias } });
        }}
      />
      <SlotLabel>=</SlotLabel>
      <ExprChain
        stepId={step.id}
        expr={expr}
        frame={frame}
        onChange={(next) => {
          onChange({ ...step, expr: next });
        }}
      />
    </>
  );
}

function SortSlots({ step, frame, onChange }: SlotsProps<SortStep>): React.JSX.Element {
  const withColumns = (by: readonly string[]): void => {
    onChange({ ...step, by });
  };
  const unused = frame.find((column) => !step.by.includes(column.name));
  const directions: Choice[] = [false, true].map((descending) => ({
    label: descending ? 'descending' : 'ascending',
    current: descending === step.descending,
    reason: null,
    onPick: () => {
      onChange({ ...step, descending });
    },
  }));
  return (
    <>
      {step.by.map((column, position) => (
        <span
          // The same column can be chosen twice, so its place is part of the key.
          key={`${String(position)}:${column}`}
          className="inline-flex items-center gap-2"
        >
          <SlotLabel>{position === 0 ? 'by' : 'then by'}</SlotLabel>
          <Picker
            label={column}
            tone="column"
            choices={[
              ...columnChoices(frame, column, (chosen) => {
                withColumns(step.by.map((existing, at) => (at === position ? chosen : existing)));
              }),
              {
                label: '✕ Remove',
                current: false,
                reason: step.by.length === 1 ? 'the only column' : null,
                onPick: () => {
                  withColumns(step.by.filter((_existing, at) => at !== position));
                },
              },
            ]}
          />
        </span>
      ))}
      {unused !== undefined && (
        <button
          type="button"
          className="cursor-pointer rounded-md px-1.5 text-[13px] leading-[22px] text-muted hover:bg-hover hover:text-ink"
          onClick={() => {
            withColumns([...step.by, unused.name]);
          }}
        >
          + then by
        </button>
      )}
      <Picker
        label={step.descending ? 'descending' : 'ascending'}
        tone="plain"
        choices={directions}
      />
    </>
  );
}

function FilterSlots({ step, frame, onChange }: SlotsProps<FilterStep>): React.JSX.Element {
  return (
    <>
      <SlotLabel>where</SlotLabel>
      <Picker
        label={step.column}
        tone="column"
        choices={columnChoices(frame, step.column, (column) => {
          onChange({ ...step, column, value: distinctValues(frame, column)[0] ?? null });
        })}
      />
      <Picker
        label={step.comparison}
        tone="operator"
        choices={COMPARISONS.map((comparison) => ({
          label: comparison,
          current: comparison === step.comparison,
          reason: null,
          onPick: () => {
            onChange({ ...step, comparison });
          },
        }))}
      />
      <Picker
        label={literalText(step.value)}
        tone="term"
        choices={distinctValues(frame, step.column).map((value) => ({
          label: literalText(value),
          current: value === step.value,
          reason: null,
          onPick: () => {
            onChange({ ...step, value });
          },
        }))}
      />
    </>
  );
}

function Slots({ step, frame, onChange }: SlotsProps<Step>): React.JSX.Element {
  switch (step.verb) {
    case 'with_columns':
      return <AddColumnSlots step={step} frame={frame} onChange={onChange} />;
    case 'sort':
      return <SortSlots step={step} frame={frame} onChange={onChange} />;
    case 'filter':
      return <FilterSlots step={step} frame={frame} onChange={onChange} />;
    default:
      return <></>;
  }
}

function Outcome({
  step,
  frame,
  result,
}: {
  readonly step: Step;
  readonly frame: Frame;
  readonly result: StepResult;
}): React.JSX.Element | null {
  if (result.problem !== null) {
    return <p className="m-0 mt-1.5 ml-11 text-[13px] text-danger">{result.problem}</p>;
  }
  if (step.verb !== 'with_columns') return null;
  const dtype = checkExpr(step.expr, frame).dtypes.at(-1);
  if (dtype === undefined) return null;
  return (
    <p className="m-0 mt-1.5 ml-11 flex items-center gap-2 text-[13px]">
      <SlotLabel>returns</SlotLabel>
      <span className="font-mono text-[12.5px] text-ready">{dtype}</span>
    </p>
  );
}

/** What a step card shows and reports. */
export interface StepCardProps {
  readonly step: Step;
  readonly index: number;
  /** The frame this step is given. */
  readonly frame: Frame;
  readonly result: StepResult;
  readonly selected: boolean;
  readonly onSelect: () => void;
  readonly onChange: (step: Step) => void;
  readonly onRemove: () => void;
}

/** Draws one step as a card that can be edited, selected, dragged and removed. */
export function StepCard({
  step,
  index,
  frame,
  result,
  selected,
  onSelect,
  onChange,
  onRemove,
}: StepCardProps): React.JSX.Element {
  const { ref, handleRef, isDragSource } = useSortable({
    id: step.id,
    index,
    group: STEP_GROUP,
    type: 'step',
    accept: 'step',
  });
  return (
    <div
      ref={ref}
      className={cx(
        'rounded-[10px] border bg-row px-3 py-2',
        selected ? 'border-accent' : 'border-line',
        result.problem !== null && 'border-danger',
        isDragSource && 'opacity-60',
      )}
      onPointerDownCapture={onSelect}
      onFocusCapture={onSelect}
    >
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5">
        <button
          ref={handleRef}
          type="button"
          title="Drag to reorder"
          aria-label={`Step ${String(index + 1)}: drag to reorder`}
          className="inline-flex cursor-grab touch-none items-center gap-0.5 rounded-md px-0.5 text-muted select-none hover:bg-hover hover:text-ink active:cursor-grabbing"
        >
          <span aria-hidden="true" className="text-sm leading-7">
            ⠿
          </span>
          <span className="min-w-3 text-right text-xs leading-7">{index + 1}</span>
        </button>
        <Picker
          label={VERB_LABELS[step.verb]}
          tone="verb"
          choices={verbChoices(step.verb, (verb) => {
            onChange({ ...defaultStep(verb, frame), id: step.id });
          })}
        />
        <Slots step={step} frame={frame} onChange={onChange} />
        <button
          type="button"
          title="Remove this step"
          aria-label={`Remove step ${String(index + 1)}`}
          className="ml-auto size-6 cursor-pointer rounded-md border border-transparent text-xs text-muted hover:border-line-strong hover:text-ink"
          onClick={onRemove}
        >
          ✕
        </button>
      </div>
      <Outcome step={step} frame={frame} result={result} />
    </div>
  );
}
