/**
 * @file One step of the list, drawn as a single row: its number, its verb,
 * its slots and what it returns. Pointing at the row shows its tools: the
 * number becomes a grip to drag the step by, and a cross removes it.
 */

import { useSortable } from '@dnd-kit/react/sortable';

import type { Choice } from '@dageist/design';
import { CommitField, Hint, Picker, cx } from '@dageist/design';

import { ChainEditor, ConditionSlots } from './ChainEditor';
import { SubListHost } from './SubList';
import { columnChoices } from './choices';
import { formatCell } from './format';
import type { Frame } from './model/frame';
import { distinctValues } from './model/frame';
import type {
  ColumnsFrom,
  FilterStep,
  JoinStep,
  PivotStep,
  SortStep,
  Step,
  StepResult,
  Verb,
  WithColumnsStep,
} from './model/steps';
import {
  AGGREGATES,
  COLUMNS_FROM,
  HOWS,
  VERBS,
  defaultStep,
  stepBreak,
  stepText,
} from './model/steps';
import { checkChain } from './model/source';
import { TABLES, findTable } from './model/tables';

/** The sortable group every step row belongs to. */
export const STEP_GROUP = 'steps';

/** What each step is called: Polars' own word where it has one, in a plain phrase. */
const VERB_LABELS: Readonly<Record<Verb, string>> = {
  with_columns: 'Add column',
  sort: 'Sort table',
  filter: 'Filter table',
  join: 'Join table',
  pivot: 'Pivot table',
};

/** The colour each verb is written in, so the kind of a step shows at a glance. */
const VERB_COLOURS: Readonly<Record<Verb, string>> = {
  with_columns: 'text-accent',
  sort: 'text-ready',
  filter: 'text-column-ink',
  join: 'text-table-ink',
  pivot: 'text-operator',
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
          onChange({ ...step, expr: { source: expr.source, methods: expr.methods, alias } });
        }}
      />
      <SlotLabel>=</SlotLabel>
      <ChainEditor
        chainId={step.id}
        chain={expr}
        frame={frame}
        onChange={(chain) => {
          onChange({ ...step, expr: { ...chain, alias: expr.alias } });
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
      <ConditionSlots
        condition={step.condition}
        frame={frame}
        onChange={(condition) => {
          onChange({ ...step, condition });
        }}
      />
    </>
  );
}

function JoinSlots({ step, frame, onChange }: SlotsProps<JoinStep>): React.JSX.Element {
  const table = findTable(step.table);
  const leftDtype = frame.find((column) => column.name === step.leftOn)?.dtype;
  const described = (table?.frame ?? []).map((column) => `${column.name} · ${column.dtype}`);
  return (
    <>
      <Hint
        lines={[`${step.table}: ${String(table?.frame[0]?.cells.length ?? 0)} rows`, ...described]}
        code
      >
        <Picker
          label={step.table}
          tone="table"
          choices={TABLES.map((candidate) => ({
            label: candidate.name,
            current: candidate.name === step.table,
            reason: null,
            onPick: () => {
              // The key belonged to the old table, so start from the new table's first column.
              onChange({ ...step, table: candidate.name, rightOn: candidate.frame[0]?.name ?? '' });
            },
          }))}
        />
      </Hint>
      <SlotLabel>on</SlotLabel>
      <Picker
        label={step.leftOn}
        tone="column"
        choices={columnChoices(frame, step.leftOn, (leftOn) => {
          onChange({ ...step, leftOn });
        })}
      />
      <span className="px-0.5 font-mono text-[12.5px] font-semibold text-operator">==</span>
      <Picker
        label={step.rightOn}
        tone="table"
        choices={(table?.frame ?? []).map((column) => ({
          label: `${column.name} · ${column.dtype}`,
          current: column.name === step.rightOn,
          reason:
            leftDtype === undefined || column.dtype === leftDtype ? null : 'a different dtype',
          onPick: () => {
            onChange({ ...step, rightOn: column.name });
          },
        }))}
      />
      <SlotLabel>how</SlotLabel>
      <Picker
        label={step.how}
        tone="term"
        choices={HOWS.map((how) => ({
          label: how,
          current: how === step.how,
          reason: null,
          onPick: () => {
            onChange({ ...step, how });
          },
        }))}
      />
    </>
  );
}

function PivotSlots({ step, frame, onChange }: SlotsProps<PivotStep>): React.JSX.Element {
  const column = (
    label: string,
    current: string,
    update: (name: string) => PivotStep,
  ): React.JSX.Element => (
    <>
      <SlotLabel>{label}</SlotLabel>
      <Picker
        label={current}
        tone="column"
        choices={columnChoices(frame, current, (name) => {
          onChange(update(name));
        })}
      />
    </>
  );
  return (
    <>
      {column('on', step.on, (on) => ({
        ...step,
        on,
        // Pinned columns are values of the old column, so pin the new column's instead.
        pinned: step.columnsFrom === 'pinned' ? distinctValues(frame, on) : [],
      }))}
      {column('index', step.index, (index) => ({ ...step, index }))}
      {column('values', step.values, (values) => ({ ...step, values }))}
      <SlotLabel>aggregate</SlotLabel>
      <Picker
        label={step.aggregate}
        tone="term"
        choices={AGGREGATES.map((aggregate) => ({
          label: aggregate,
          current: aggregate === step.aggregate,
          reason: null,
          onPick: () => {
            onChange({ ...step, aggregate });
          },
        }))}
      />
      {step.columnsFrom === 'pinned' && (
        <>
          <SlotLabel>columns</SlotLabel>
          <span className="font-mono text-[12.5px]">
            {step.pinned.map((value) => String(value)).join(', ')}
          </span>
        </>
      )}
    </>
  );
}

/** What each way of finding a pivot's columns is called in the row and in its menu. */
const COLUMNS_FROM_LABELS: Readonly<Record<ColumnsFrom, readonly [string, string]>> = {
  collect: ['needs a collect', 'Collect here'],
  sink: ['sinks first', 'Sink first, in a node of its own'],
  pinned: ['stays lazy', 'Pin the columns and stay lazy'],
};

/** What the band above a step says when the lazy query has to stop there. */
const BREAK_NOTES: Readonly<Record<'collect' | 'sink', string>> = {
  collect: 'The lazy query stops here. The rows so far are loaded into memory.',
  sink: 'The rows so far are written to Parquet first. This step would move to a node of its own.',
};

interface PivotOutcomeProps {
  readonly step: PivotStep;
  readonly frame: Frame;
  readonly result: StepResult;
  readonly onChange: (step: Step) => void;
}

/** Draws what a pivot returns and how it finds its columns, with the choices for that. */
function PivotOutcome({ step, frame, result, onChange }: PivotOutcomeProps): React.JSX.Element {
  const fromData = step.columnsFrom !== 'pinned';
  return (
    <span className="inline-flex shrink-0 items-center gap-1 pl-2 text-xs text-muted">
      <Hint
        lines={[
          `A pivot returns one column for each value of "${step.on}".`,
          'Polars can plan that lazily only when those values are given up front.',
        ]}
        code={false}
      >
        <Picker
          label={COLUMNS_FROM_LABELS[step.columnsFrom][0]}
          tone="plain"
          className={cx('text-xs', fromData ? 'text-operator' : 'text-ready')}
          choices={COLUMNS_FROM.map((columnsFrom) => ({
            label: COLUMNS_FROM_LABELS[columnsFrom][1],
            current: columnsFrom === step.columnsFrom,
            reason: null,
            onPick: () => {
              onChange({
                ...step,
                columnsFrom,
                pinned: columnsFrom === 'pinned' ? distinctValues(frame, step.on) : [],
              });
            },
          }))}
        />
      </Hint>
      {result.problem === null && (
        <Hint
          lines={[
            result.added.join(', '),
            ...(fromData
              ? [
                  'These come from the data. Here they come from the sample rows, so a full run may return others.',
                ]
              : ['Only the pinned values become columns.']),
          ]}
          code={false}
        >
          <span>
            returns{' '}
            <span className="font-mono text-[12.5px] text-ready">
              {String(result.frame.length)} columns
            </span>
          </span>
        </Hint>
      )}
    </span>
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
    case 'join':
      return <JoinSlots step={step} frame={frame} onChange={onChange} />;
    case 'pivot':
      return <PivotSlots step={step} frame={frame} onChange={onChange} />;
    default:
      return <></>;
  }
}

/** How many values of a new column its hover note shows. */
const PEEK = 4;

/** Returns the first values of the column called `name` in `result`, as one line of text. */
function peek(result: StepResult, name: string): string[] {
  const column = result.frame.find((candidate) => candidate.name === name);
  if (column === undefined) return [];
  const values = column.cells.slice(0, PEEK).map((cell) => formatCell(cell, column.dtype));
  return [`${values.join(', ')}${column.cells.length > PEEK ? ', …' : ''}`];
}

/** Returns the dtype an Add column step returns, or `null` for other steps and broken chains. */
function returnedDtype(step: Step, frame: Frame, result: StepResult): string | null {
  if (step.verb !== 'with_columns' || result.problem !== null) return null;
  return checkChain(step.expr, frame).dtypes.at(-1) ?? null;
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
  /** Adds a new step of the chosen verb directly after this one. */
  readonly onInsertAfter: (verb: Verb) => void;
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
  onInsertAfter,
}: StepCardProps): React.JSX.Element {
  const { ref, handleRef, isDragSource } = useSortable({
    id: step.id,
    index,
    group: STEP_GROUP,
    type: 'step',
    accept: 'step',
  });
  const dtype = returnedDtype(step, frame, result);
  const broken = stepBreak(step);
  const edge = result.problem === null ? 'var(--color-accent)' : 'var(--color-danger)';
  return (
    <div
      ref={ref}
      className={cx(
        'group/step relative border-b border-line bg-panel first:rounded-t-lg last:rounded-b-lg last:border-b-0',
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
      {broken !== null && (
        <div className="flex items-center gap-2 border-b border-dashed border-operator px-2 py-0.5 text-[11px] text-muted">
          <span className="rounded-sm bg-invalid px-1.5 font-mono font-semibold text-operator">
            {broken}
          </span>
          {BREAK_NOTES[broken]}
        </div>
      )}
      <SubListHost outletClassName="mr-2 mb-1 ml-[132px] border-l-2 border-line-strong pl-2">
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
          <Hint lines={[stepText(step)]} code>
            <Picker
              label={VERB_LABELS[step.verb]}
              tone="verb"
              className={cx('w-[92px] shrink-0 justify-start', VERB_COLOURS[step.verb])}
              choices={verbChoices(step.verb, (verb) => {
                onChange({ ...defaultStep(verb, frame), id: step.id });
              })}
            />
          </Hint>
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-0.5 gap-y-1">
            <Slots step={step} frame={frame} onChange={onChange} />
          </div>
          {dtype !== null && (
            <Hint lines={peek(result, result.added[0] ?? '')} code>
              <span className="shrink-0 pl-2 text-xs text-muted">
                returns <span className="font-mono text-[12.5px] text-ready">{dtype}</span>
              </span>
            </Hint>
          )}
          {step.verb === 'pivot' && (
            <PivotOutcome step={step} frame={frame} result={result} onChange={onChange} />
          )}
          {step.verb === 'join' && result.problem === null && (
            <Hint
              lines={result.added.flatMap((name) => [
                `${name} · ${result.frame.find((column) => column.name === name)?.dtype ?? ''}`,
                ...peek(result, name),
              ])}
              code
            >
              <span className="shrink-0 pl-2 text-xs text-muted">
                adds{' '}
                <span className="font-mono text-[12.5px] text-ready">
                  {result.added.length === 1
                    ? result.added[0]
                    : `${String(result.added.length)} columns`}
                </span>
              </span>
            </Hint>
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
      </SubListHost>
      {result.problem !== null && (
        <p className="m-0 pb-1.5 pl-[132px] text-[13px] text-danger">{result.problem}</p>
      )}
      {/* Sits on the line under the row, where a new step would go. */}
      <span className="absolute -bottom-[9px] left-[11px] z-10 opacity-0 group-focus-within/step:opacity-100 group-hover/step:opacity-100">
        <Picker
          label="+"
          tone="plain"
          className="size-[18px] justify-center rounded-full border border-line-strong bg-panel px-0 text-[12px] leading-none font-bold text-muted hover:text-accent"
          choices={verbChoices(null, onInsertAfter)}
        />
      </span>
    </div>
  );
}
