/**
 * @file One step of the list, drawn as a single row: its number, its verb,
 * its slots and what it returns. Pointing at the row shows its tools: the
 * number becomes a grip to drag the step by, and a cross removes it.
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

/** The sortable group every step row belongs to. */
export const STEP_GROUP = 'steps';

/** What each step is called: Polars' own word where it has one, in a plain phrase. */
const VERB_LABELS: Readonly<Record<Verb, string>> = {
  with_columns: 'Add column',
  sort: 'Sort table',
  filter: 'Filter table',
};

/** The colour each verb is written in, so the kind of a step shows at a glance. */
const VERB_COLOURS: Readonly<Record<Verb, string>> = {
  with_columns: 'text-accent',
  sort: 'text-ready',
  filter: 'text-column-ink',
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
  return <span className="px-0.5 text-xs text-muted">{children}</span>;
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
      <SlotLabel>by</SlotLabel>
      {step.by.map((column, position) => (
        <Picker
          // The same column can be chosen twice, so its place is part of the key.
          key={`${String(position)}:${column}`}
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
      ))}
      {unused !== undefined && (
        <button
          type="button"
          aria-label="Sort by another column as well"
          title="Sort by another column as well"
          className="inline-flex h-[22px] w-4 cursor-pointer items-center justify-center rounded-sm text-[13px] font-bold text-muted opacity-0 group-focus-within/step:opacity-100 group-hover/step:opacity-100 hover:bg-hover hover:text-accent"
          onClick={() => {
            withColumns([...step.by, unused.name]);
          }}
        >
          +
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

/** Returns the dtype an Add column step returns, or `null` for other steps and broken chains. */
function returnedDtype(step: Step, frame: Frame, result: StepResult): string | null {
  if (step.verb !== 'with_columns' || result.problem !== null) return null;
  return checkExpr(step.expr, frame).dtypes.at(-1) ?? null;
}

/** What a step row shows and reports. */
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

/** Draws one step as a row that can be edited, selected, dragged and removed. */
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
  const dtype = returnedDtype(step, frame, result);
  const edge = result.problem === null ? 'var(--color-accent)' : 'var(--color-danger)';
  return (
    <div
      ref={ref}
      className={cx(
        'group/step border-b border-line bg-panel last:border-b-0',
        selected && 'bg-row',
        !selected && 'hover:bg-row/50',
        isDragSource && 'opacity-60',
      )}
      style={
        selected || result.problem !== null ? { boxShadow: `inset 2px 0 0 ${edge}` } : undefined
      }
      onPointerDownCapture={onSelect}
      onFocusCapture={onSelect}
    >
      <div className="flex min-h-9 items-center gap-1 py-1 pr-2 pl-1">
        <button
          ref={handleRef}
          type="button"
          title="Drag to reorder"
          aria-label={`Step ${String(index + 1)}: drag to reorder`}
          className="inline-flex h-7 w-7 shrink-0 cursor-grab touch-none items-center justify-center rounded-md text-xs text-muted select-none hover:bg-hover hover:text-ink active:cursor-grabbing"
        >
          <span className="group-hover/step:hidden">{index + 1}</span>
          <span aria-hidden="true" className="hidden text-sm group-hover/step:inline">
            ⠿
          </span>
        </button>
        <Picker
          label={VERB_LABELS[step.verb]}
          tone="verb"
          className={cx('w-[92px] shrink-0 justify-start', VERB_COLOURS[step.verb])}
          choices={verbChoices(step.verb, (verb) => {
            onChange({ ...defaultStep(verb, frame), id: step.id });
          })}
        />
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-0.5 gap-y-1">
          <Slots step={step} frame={frame} onChange={onChange} />
        </div>
        {dtype !== null && (
          <span className="shrink-0 pl-2 text-xs text-muted">
            returns <span className="font-mono text-[12.5px] text-ready">{dtype}</span>
          </span>
        )}
        <button
          type="button"
          title="Remove this step"
          aria-label={`Remove step ${String(index + 1)}`}
          className="size-6 shrink-0 cursor-pointer rounded-md text-xs text-muted opacity-0 group-focus-within/step:opacity-100 group-hover/step:opacity-100 hover:bg-hover hover:text-danger"
          onClick={onRemove}
        >
          ✕
        </button>
      </div>
      {result.problem !== null && (
        <p className="m-0 pb-1.5 pl-[132px] text-[13px] text-danger">{result.problem}</p>
      )}
    </div>
  );
}
