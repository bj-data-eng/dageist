/**
 * @file One step of the list, written as a line of code: its number, its
 * name, what it takes, and at the end of the row what it returns. Nothing
 * about the row changes when it is pointed at. The step's name is the word
 * that names it: its list changes what the step does, and ends with adding
 * a step after it and removing it. The number drags the step to another
 * place. What a step takes that is more than one thing is set out on lines
 * beneath it.
 */

import { useSortable } from '@dnd-kit/react/sortable';
import { useCallback, useContext, useState } from 'react';

import type { Entry, OfferGroup } from '@dageist/design';
import {
  Hint,
  ManySlot,
  OfferList,
  Peek,
  Piece,
  Popover,
  Slot,
  TypedBlock,
  Word,
  cx,
} from '@dageist/design';

import { useCatalogue } from './CatalogueProvider';
import { FreshContext } from './Lists';
import {
  ColumnDrawing,
  ColumnsDrawing,
  OptionRows,
  TableDrawing,
  optionOffers,
} from './Parameters';
import { ConditionBlocks, ValueSlot } from './Values';
import { formatCell } from './format';
import { counted, fieldOffers, matches, offer, titled } from './offers';
import type { Cell, Frame } from './model/frame';
import { distinctValues, findColumn, keysFit } from './model/frame';
import type {
  ColumnsFrom,
  FilterStep,
  JoinStep,
  PivotStep,
  RenameStep,
  SelectStep,
  SortStep,
  Step,
  StepResult,
  Verb,
  WithColumnsStep,
} from './model/steps';
import {
  AGGREGATES,
  COLUMNS_FROM,
  VERBS,
  defaultStep,
  skippedStepOptions,
  stepBreak,
  stepItems,
  stepOptionParameters,
  stepText,
} from './model/steps';
import type { Catalogue } from './model/catalogue';
import { verbEntry, verbId } from './model/catalogue';
import { findTable } from './model/tables';
import { VERB_COLOURS, verbHead, verbNote } from './verbs';

/** The sortable group every step row belongs to. */
export const STEP_GROUP = 'steps';

/** Lists the steps that can be added or changed to, in the words of the catalogue. */
export function verbOffers(
  catalogue: Catalogue,
  typed: string,
  onPick: (verb: Verb) => void,
): OfferGroup[] {
  return [
    titled(
      'Step',
      VERBS.filter((verb) =>
        matches(typed, verbHead(catalogue, verb), verbNote(catalogue, verb)),
      ).map((verb) =>
        offer(
          verbHead(catalogue, verb),
          'ink',
          verbNote(catalogue, verb),
          () => {
            onPick(verb);
          },
          {
            look: 'verb',
            // A verb the storyboard has and the catalogue lacks cannot be chosen.
            reason: verbEntry(catalogue, verb) === null ? 'not in the catalogue' : null,
          },
        ),
      ),
    ),
  ];
}

interface HeadProps<S extends Step> {
  readonly step: S;
  readonly frame: Frame;
  readonly onChange: (step: Step) => void;
}

function AddColumnHead({ step, frame, onChange }: HeadProps<WithColumnsStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  return (
    <>
      <TypedBlock
        value={step.alias}
        kind="name"
        label="Name of the new column"
        wrong={step.alias.trim() === ''}
        focusNow={fresh.id === step.id}
        accepts={(typed) => typed.trim() !== ''}
        onCommit={(alias) => {
          onChange({ ...step, alias: alias.trim() });
        }}
      />
      <Word>=</Word>
      <ValueSlot
        value={step.value}
        frame={frame}
        label={`What ${step.alias === '' ? 'the column' : step.alias} holds`}
        lists={false}
        expected={null}
        onChange={(value) => {
          onChange({ ...step, value });
        }}
      />
    </>
  );
}

function SortHead({ step, frame, onChange }: HeadProps<SortStep>): React.JSX.Element {
  return (
    <>
      <Word>by</Word>
      <ColumnsDrawing
        frame={frame}
        names={step.by}
        empty="no fields"
        describedAs="The fields the rows are ordered by"
        openId={step.id}
        onChange={(by) => {
          onChange({ ...step, by });
        }}
      />
    </>
  );
}

function SelectHead({ step, frame, onChange }: HeadProps<SelectStep>): React.JSX.Element {
  return (
    <ColumnsDrawing
      frame={frame}
      names={step.columns}
      empty="no fields"
      describedAs="The fields that are kept"
      openId={step.id}
      onChange={(columns) => {
        onChange({ ...step, columns });
      }}
    />
  );
}

function RenameHead({ step, frame, onChange }: HeadProps<RenameStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const entries: Entry[] = step.names.map(({ from, to }) => ({
    id: from,
    wrong: findColumn(frame, from) === null || to === '',
    blocks: [
      { label: from, tone: 'column', look: 'plain' },
      { label: to === '' ? 'name' : to, tone: 'ink', look: to === '' ? 'none' : 'plain' },
    ],
  }));
  return (
    <ManySlot
      entries={entries}
      empty="no fields"
      describedAs="The fields that are given new names"
      placeholder="Search the fields"
      wide
      openNow={fresh.id === step.id}
      offers={(typed) => [
        counted(
          'Fields',
          fieldOffers(frame, typed, {
            reasonFor: () => null,
            chosen: step.names.map((naming) => naming.from),
            renderExtra: (name) => {
              const naming = step.names.find((candidate) => candidate.from === name);
              if (naming === undefined) return null;
              // A setting that can be anything is typed in a field inside the list.
              return (
                <>
                  <span className="dg-arrow">→</span>
                  <input
                    type="text"
                    aria-label={`New name for ${name}`}
                    className="dg-given"
                    spellCheck={false}
                    value={naming.to}
                    onChange={(event) => {
                      const to = event.currentTarget.value;
                      onChange({
                        ...step,
                        names: step.names.map((other) =>
                          other.from === name ? { from: name, to } : other,
                        ),
                      });
                    }}
                  />
                </>
              );
            },
            onPick: (name) => {
              const has = step.names.some((naming) => naming.from === name);
              onChange({
                ...step,
                names: has
                  ? step.names.filter((naming) => naming.from !== name)
                  : [...step.names, { from: name, to: name }],
              });
            },
          }),
        ),
      ]}
    />
  );
}

function FilterHead({ step, frame, onChange }: HeadProps<FilterStep>): React.JSX.Element {
  return (
    <ConditionBlocks
      condition={step.condition}
      frame={frame}
      tone="verb"
      onChange={(condition) => {
        onChange({ ...step, condition });
      }}
    />
  );
}

function JoinHead({ step, frame, onChange }: HeadProps<JoinStep>): React.JSX.Element {
  const table = findTable(step.table);
  return (
    <>
      <TableDrawing
        describedAs="The table that is joined"
        value={step.table}
        onChange={(name) => {
          const joined = findTable(name);
          // The keys belonged to the old table, so start from the new table's first field.
          onChange({ ...step, table: name, rightOn: [joined?.frame[0]?.name ?? ''] });
        }}
      />
      <Word>on</Word>
      <ColumnsDrawing
        frame={frame}
        names={step.leftOn}
        empty="field"
        describedAs="The fields of these rows that are matched"
        onChange={(leftOn) => {
          onChange({ ...step, leftOn });
        }}
      />
      <span data-tone="compare" data-look="compare" className="dg-block dg-fixed">
        ==
      </span>
      <ColumnsDrawing
        frame={table?.frame ?? []}
        names={step.rightOn}
        empty="field"
        describedAs={`The fields of ${step.table} that are matched`}
        reasonFor={(dtype) => {
          // Each key is matched with the one at its place on the other side.
          const left = findColumn(frame, step.leftOn[step.rightOn.length] ?? '')?.dtype;
          return left === undefined || keysFit(dtype, left) ? null : 'a different dtype';
        }}
        onChange={(rightOn) => {
          onChange({ ...step, rightOn });
        }}
      />
    </>
  );
}

/** Returns how a value that becomes a column is written. */
function cellLabel(cell: Cell): string {
  return cell === null ? 'None' : String(cell);
}

function PivotHead({ step, frame, onChange }: HeadProps<PivotStep>): React.JSX.Element {
  const field = (
    word: string,
    current: string,
    describedAs: string,
    update: (name: string) => PivotStep,
  ): React.JSX.Element => (
    <>
      <Word>{word}</Word>
      <ColumnDrawing
        frame={frame}
        describedAs={describedAs}
        value={current}
        onChange={(name) => {
          onChange(update(name));
        }}
      />
    </>
  );
  const values = distinctValues(frame, step.on);
  return (
    <>
      {field('on', step.on, 'The field whose values become columns', (on) => ({
        ...step,
        on,
        // Pinned columns are values of the old field, so pin the new field's instead.
        pinned: step.columnsFrom === 'pinned' ? distinctValues(frame, on) : [],
      }))}
      {field('index', step.index, 'The field that stays as rows', (index) => ({
        ...step,
        index,
      }))}
      {field('values', step.values, 'The field the cells are filled from', (chosen) => ({
        ...step,
        values: chosen,
      }))}
      <Word>aggregate</Word>
      <Slot
        label={step.aggregate}
        tone="value"
        look="plain"
        describedAs="How the values that land in one cell are combined"
        placeholder="Search"
        offers={(typed) => [
          titled(
            'Aggregate',
            AGGREGATES.filter((aggregate) => matches(typed, aggregate)).map((aggregate) =>
              offer(aggregate, 'value', '', () => {
                onChange({ ...step, aggregate });
              }),
            ),
          ),
        ]}
      />
      {step.columnsFrom === 'pinned' && (
        <>
          <Word>columns</Word>
          <ManySlot
            entries={step.pinned.map((value) => ({
              id: cellLabel(value),
              wrong: false,
              blocks: [{ label: cellLabel(value), tone: 'value', look: 'plain' }],
            }))}
            empty="no values"
            describedAs={`The values of ${step.on} that become columns`}
            placeholder="Search the values"
            offers={(typed) => [
              counted(
                `Values of ${step.on}`,
                [...step.pinned.filter((value) => !values.includes(value)), ...values]
                  .filter((value) => matches(typed, cellLabel(value)))
                  .map((value) =>
                    offer(
                      cellLabel(value),
                      'value',
                      '',
                      () => {
                        onChange({
                          ...step,
                          pinned: step.pinned.includes(value)
                            ? step.pinned.filter((other) => other !== value)
                            : [...step.pinned, value],
                        });
                      },
                      { ticked: step.pinned.includes(value) },
                    ),
                  ),
              ),
            ]}
          />
        </>
      )}
    </>
  );
}

/** What each way of finding a pivot's columns is called in the row and in its list. */
const COLUMNS_FROM_LABELS: Readonly<Record<ColumnsFrom, readonly [string, string]>> = {
  collect: ['needs a collect', 'collect here'],
  sink: ['sinks first', 'sink first, in a node of its own'],
  pinned: ['stays lazy', 'pin the columns and stay lazy'],
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
    <>
      <Hint
        lines={[
          `A pivot returns one column for each value of "${step.on}".`,
          'Polars can plan that lazily only when those values are given up front.',
        ]}
        code={false}
      >
        <Slot
          label={COLUMNS_FROM_LABELS[step.columnsFrom][0]}
          tone={fromData ? 'compare' : 'add'}
          look="plain"
          describedAs="How the pivot finds out which columns it returns"
          placeholder="Search"
          offers={(typed) => [
            titled(
              'Its columns',
              COLUMNS_FROM.filter((from) => matches(typed, COLUMNS_FROM_LABELS[from][1])).map(
                (from) =>
                  offer(
                    COLUMNS_FROM_LABELS[from][1],
                    from === 'pinned' ? 'add' : 'compare',
                    '',
                    () => {
                      onChange({
                        ...step,
                        columnsFrom: from,
                        pinned: from === 'pinned' ? distinctValues(frame, step.on) : [],
                      });
                    },
                  ),
              ),
            ),
          ]}
        />
      </Hint>
      {result.problem === null && (
        <Peek
          describedAs="The columns the pivot returns"
          lines={[
            result.added.join(', '),
            fromData
              ? 'These come from the data. Here they come from the sample rows, so a full run may return others.'
              : 'Only the pinned values become columns.',
          ]}
          code={false}
        >
          <span className="dg-dtype">{String(result.frame.length)} columns</span>
        </Peek>
      )}
    </>
  );
}

/** How many values of a new column are shown when its dtype is pressed. */
const PEEK = 4;

/** Returns the first values of the column called `name` in `result`, as one line of text. */
function peek(result: StepResult, name: string): string[] {
  const column = findColumn(result.frame, name);
  if (column === null) return [];
  const values = column.cells.slice(0, PEEK).map((cell) => formatCell(cell, column.dtype));
  return [`${values.join(', ')}${column.cells.length > PEEK ? ', …' : ''}`];
}

interface OutcomeProps {
  readonly step: Step;
  readonly frame: Frame;
  readonly result: StepResult;
  readonly onChange: (step: Step) => void;
}

/** Draws what stands at the end of a step's row: the dtype it returns, or what it adds. */
function Outcome({ step, frame, result, onChange }: OutcomeProps): React.JSX.Element | null {
  if (step.verb === 'pivot') {
    return <PivotOutcome step={step} frame={frame} result={result} onChange={onChange} />;
  }
  if (result.problem !== null) return null;
  if (step.verb === 'with_columns') {
    const dtype = findColumn(result.frame, step.alias)?.dtype;
    if (dtype === undefined) return null;
    return (
      <Peek
        describedAs={`${step.alias} is ${dtype}: show its first values`}
        lines={peek(result, result.added[0] ?? '')}
        code
      >
        <span className="dg-dtype">{dtype}</span>
      </Peek>
    );
  }
  if (step.verb === 'join') {
    const [only] = result.added;
    return (
      <Peek
        describedAs="The columns the join adds: show their first values"
        lines={result.added.flatMap((name) => [
          `${name} · ${findColumn(result.frame, name)?.dtype ?? ''}`,
          ...peek(result, name),
        ])}
        code
      >
        <span className="text-xs text-muted">
          adds{' '}
          <span className="dg-dtype">
            {result.added.length === 1 && only !== undefined
              ? only
              : `${String(result.added.length)} columns`}
          </span>
        </span>
      </Peek>
    );
  }
  if (step.verb === 'filter') {
    const rows = result.frame[0]?.cells.length ?? 0;
    const before = frame[0]?.cells.length ?? 0;
    return (
      <span className="text-xs text-muted">
        keeps <span className="dg-dtype">{`${String(rows)} of ${String(before)} rows`}</span>
      </span>
    );
  }
  return null;
}

function Head({ step, frame, onChange }: HeadProps<Step>): React.JSX.Element | null {
  switch (step.verb) {
    case 'with_columns':
      return <AddColumnHead step={step} frame={frame} onChange={onChange} />;
    case 'filter':
      return <FilterHead step={step} frame={frame} onChange={onChange} />;
    case 'sort':
      return <SortHead step={step} frame={frame} onChange={onChange} />;
    case 'select':
      return <SelectHead step={step} frame={frame} onChange={onChange} />;
    case 'rename':
      return <RenameHead step={step} frame={frame} onChange={onChange} />;
    case 'join':
      return <JoinHead step={step} frame={frame} onChange={onChange} />;
    case 'pivot':
      return <PivotHead step={step} frame={frame} onChange={onChange} />;
    default:
      return null;
  }
}

/** What a step row shows and reports. */
export interface StepRowProps {
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
export function StepRow({
  step,
  index,
  frame,
  result,
  selected,
  onSelect,
  onChange,
  onRemove,
  onInsertAfter,
}: StepRowProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const { ref, handleRef, isDragSource } = useSortable({
    id: step.id,
    index,
    group: STEP_GROUP,
    type: 'step',
    accept: 'step',
  });
  const [adding, setAdding] = useState<HTMLElement | null>(null);
  const closeList = useCallback(() => {
    setAdding(null);
  }, []);
  const broken = stepBreak(step);
  const number = String(index + 1);
  const name = verbHead(catalogue, step.verb);
  const entry = verbEntry(catalogue, step.verb);
  const missing = entry === null;
  const parameters = stepOptionParameters(catalogue, step.verb);
  return (
    <div
      ref={ref}
      data-step-id={step.id}
      className={cx(
        'dg-ground border-b border-line bg-panel first:rounded-t-lg last:rounded-b-lg last:border-b-0',
        VERB_COLOURS[step.verb],
        selected && 'bg-row [--ground:var(--color-row)]',
        isDragSource && 'opacity-60',
      )}
      onPointerDownCapture={onSelect}
      onFocusCapture={onSelect}
    >
      {broken !== null && (
        <div className="flex items-center gap-2 border-b border-dashed border-operator px-3 py-0.5 text-[11px] text-muted">
          <span className="rounded-sm bg-invalid px-1.5 font-mono font-semibold text-operator">
            {broken}
          </span>
          {BREAK_NOTES[broken]}
        </div>
      )}
      <div className="flex items-baseline gap-3 py-1 pr-4 pl-2">
        <button
          ref={handleRef}
          type="button"
          title="Drag to reorder"
          aria-label={`Step ${number}: drag to reorder`}
          className="w-6 shrink-0 cursor-grab touch-none text-right font-mono text-[11px] text-muted select-none active:cursor-grabbing"
        >
          {number}
        </button>
        <div data-dg-flow className="dg-code">
          {/* The step's name is the word that names it: its list adds a step after and removes it. */}
          <Piece
            remove={{ label: `Remove ${name}`, onPress: onRemove }}
            add={{ label: `Add a step after ${name}`, onPress: setAdding }}
          >
            <Hint lines={[stepText(catalogue, step)]} code>
              <Slot
                label={name}
                tone="verb"
                look="verb"
                names
                describedAs={`Step ${number}: ${name}. Change what the step does`}
                placeholder="Search the steps"
                offers={(typed) => [
                  // The parameters the step can be given, then the steps it can be changed to.
                  ...(parameters.length === 0
                    ? []
                    : [
                        optionOffers(parameters, step.options, typed, (options, added) => {
                          onChange({ ...step, options });
                          if (added !== null) fresh.mark(`${step.id}:${added}`);
                        }),
                      ]),
                  ...verbOffers(catalogue, typed, (verb) => {
                    if (verb !== step.verb) {
                      onChange({ ...defaultStep(verb, frame), id: step.id });
                    }
                  }),
                ]}
              />
            </Hint>
          </Piece>
          <Head step={step} frame={frame} onChange={onChange} />
          <OptionRows
            ownerId={step.id}
            entry={entry}
            parameters={parameters}
            options={step.options}
            around={{ frame, items: (of) => stepItems(step, of) }}
            skipped={skippedStepOptions(catalogue, step)}
            layout="lines"
            onChange={(options) => {
              onChange({ ...step, options });
            }}
          />
          {missing && (
            <span className="dg-line font-sans text-xs text-wrong">
              {`The catalogue has no entry ${verbId(step.verb)}, so this step has no words to show.`}
            </span>
          )}
          {result.problem !== null && (
            <span className="dg-line font-sans text-xs text-wrong">{result.problem}</span>
          )}
        </div>
        <span className="shrink-0 pl-3 text-xs text-muted">
          <Outcome step={step} frame={frame} result={result} onChange={onChange} />
        </span>
      </div>
      {adding !== null && (
        <Popover anchor={adding} onDismiss={closeList}>
          <OfferList
            placeholder="Search the steps"
            groups={(typed) => verbOffers(catalogue, typed, onInsertAfter)}
            keepOpen={false}
            wide={false}
            onDone={closeList}
          />
        </Popover>
      )}
    </div>
  );
}
