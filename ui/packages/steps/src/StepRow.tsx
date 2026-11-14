/**
 * @file One step of the list: its number, its head and what it returns. The
 * head is a bracketed piece in the step's colour that holds the verb and
 * what the step takes. Pointing at the head parts its brackets: a cross
 * removes the step and a plus adds one after it. Pointing at the row turns
 * the number into a grip to drag the step by. What a step takes that is more
 * than one thing is a list under the row.
 */

import { useSortable } from '@dnd-kit/react/sortable';
import { useCallback, useContext, useState } from 'react';

import type { Entry, OfferGroup } from '@dageist/design';
import {
  Equals,
  Hint,
  ManySlot,
  OfferList,
  Piece,
  Popover,
  Slot,
  TypedBlock,
  Word,
  cx,
} from '@dageist/design';

import { ChainBlocks } from './Chain';
import { FreshContext, ListOwner, useLineNumbers } from './Lists';
import { ConditionBlocks } from './Values';
import { formatCell } from './format';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
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
  filter: 'Filter table',
  sort: 'Sort table',
  select: 'Select',
  rename: 'Rename',
  join: 'Join table',
  pivot: 'Pivot table',
};

/** What each step does, for someone choosing one. */
const VERB_NOTES: Readonly<Record<Verb, string>> = {
  with_columns: 'a new column from a chain',
  filter: 'the rows where a condition holds',
  sort: 'the rows in the order of some fields',
  select: 'some of the columns',
  rename: 'columns under new names',
  join: 'columns from another table, by a key',
  pivot: 'the values of a field as columns',
};

/** The colour each step is written in, so the kind of a step shows at a glance. */
const VERB_COLOURS: Readonly<Record<Verb, string>> = {
  with_columns: '[--verb:var(--color-verb-add)]',
  filter: '[--verb:var(--color-verb-filter)]',
  sort: '[--verb:var(--color-verb-sort)]',
  select: '[--verb:var(--color-verb-select)]',
  rename: '[--verb:var(--color-verb-rename)]',
  join: '[--verb:var(--color-verb-join)]',
  pivot: '[--verb:var(--color-verb-pivot)]',
};

/** Lists the steps that can be added or changed to. */
export function verbOffers(typed: string, onPick: (verb: Verb) => void): OfferGroup[] {
  return [
    titled(
      'Step',
      VERBS.filter((verb) => matches(typed, VERB_LABELS[verb], VERB_NOTES[verb])).map((verb) =>
        offer(
          VERB_LABELS[verb],
          'ink',
          VERB_NOTES[verb],
          () => {
            onPick(verb);
          },
          { look: 'verb' },
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

function AddColumnHead({ step, onChange }: HeadProps<WithColumnsStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const { expr } = step;
  return (
    <TypedBlock
      value={expr.alias}
      kind="name"
      label="Name of the new column"
      wrong={expr.alias.trim() === ''}
      focusNow={fresh.id === step.id}
      accepts={(typed) => typed.trim() !== ''}
      onCommit={(alias) => {
        onChange({ ...step, expr: { ...expr, alias: alias.trim() } });
      }}
    />
  );
}

function SortHead({ step, frame, onChange }: HeadProps<SortStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const entries: Entry[] = step.by.map((key) => ({
    id: key.column,
    blocks: [
      { label: key.column, tone: 'column', look: 'plain' },
      { label: key.descending ? 'descending' : 'ascending', tone: 'value', look: 'plain' },
    ],
  }));
  return (
    <>
      <Word apart={false}>by</Word>
      <ManySlot
        entries={entries}
        empty="no fields"
        describedAs="The fields the rows are ordered by"
        placeholder="Search the fields"
        wide
        openNow={fresh.id === step.id}
        offers={(typed) => [
          counted(
            'Fields',
            fieldOffers(frame, typed, {
              reasonFor: () => null,
              ticked: (name) => step.by.some((key) => key.column === name),
              renderExtra: (name) => {
                const key = step.by.find((candidate) => candidate.column === name);
                if (key === undefined) return null;
                // A setting with a few fixed answers is chosen from a list inside the list.
                return (
                  <select
                    aria-label={`Which way ${name} is ordered`}
                    className="dg-given"
                    value={key.descending ? 'descending' : 'ascending'}
                    onChange={(event) => {
                      const descending = event.currentTarget.value === 'descending';
                      onChange({
                        ...step,
                        by: step.by.map((other) =>
                          other.column === name ? { column: name, descending } : other,
                        ),
                      });
                    }}
                  >
                    <option>ascending</option>
                    <option>descending</option>
                  </select>
                );
              },
              onPick: (name) => {
                const has = step.by.some((key) => key.column === name);
                onChange({
                  ...step,
                  by: has
                    ? step.by.filter((key) => key.column !== name)
                    : [...step.by, { column: name, descending: false }],
                });
              },
            }),
          ),
        ]}
      />
    </>
  );
}

function SelectHead({ step, frame, onChange }: HeadProps<SelectStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const entries: Entry[] = step.columns.map((name) => ({
    id: name,
    blocks: [{ label: name, tone: 'column', look: 'plain' }],
  }));
  return (
    <ManySlot
      entries={entries}
      empty="no fields"
      describedAs="The fields that are kept"
      placeholder="Search the fields"
      openNow={fresh.id === step.id}
      offers={(typed) => [
        counted(
          'Fields',
          fieldOffers(frame, typed, {
            reasonFor: () => null,
            ticked: (name) => step.columns.includes(name),
            renderExtra: () => null,
            onPick: (name) => {
              onChange({
                ...step,
                columns: step.columns.includes(name)
                  ? step.columns.filter((other) => other !== name)
                  : [...step.columns, name],
              });
            },
          }),
        ),
      ]}
    />
  );
}

function RenameHead({ step, frame, onChange }: HeadProps<RenameStep>): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const entries: Entry[] = step.names.map(({ from, to }) => ({
    id: from,
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
            ticked: (name) => step.names.some((naming) => naming.from === name),
            renderExtra: (name) => {
              const naming = step.names.find((candidate) => candidate.from === name);
              if (naming === undefined) return null;
              // A setting that can be anything is typed in a field inside the list.
              return (
                <>
                  <span className="dg-offer-note">→</span>
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
      alone={false}
      onChange={(condition) => {
        onChange({ ...step, condition });
      }}
    />
  );
}

function JoinHead({ step, frame, onChange }: HeadProps<JoinStep>): React.JSX.Element {
  const table = findTable(step.table);
  const leftDtype = findColumn(frame, step.leftOn)?.dtype;
  const described = (table?.frame ?? []).map((column) => `${column.name} · ${column.dtype}`);
  return (
    <>
      <Hint
        lines={[`${step.table}: ${String(table?.frame[0]?.cells.length ?? 0)} rows`, ...described]}
        code
      >
        <Slot
          label={step.table}
          tone="table"
          look="plain"
          wrong={table === null}
          describedAs="The table that is joined"
          placeholder="Search the tables"
          offers={(typed) => [
            counted(
              'Tables',
              TABLES.filter((candidate) => matches(typed, candidate.name)).map((candidate) =>
                offer(
                  candidate.name,
                  'table',
                  `${String(candidate.frame[0]?.cells.length ?? 0)} × ${String(candidate.frame.length)}`,
                  () => {
                    // The key belonged to the old table, so start from the new table's first field.
                    onChange({
                      ...step,
                      table: candidate.name,
                      rightOn: candidate.frame[0]?.name ?? '',
                    });
                  },
                ),
              ),
            ),
          ]}
        />
      </Hint>
      <Word apart>on</Word>
      <Slot
        label={step.leftOn}
        tone="column"
        look="plain"
        wrong={findColumn(frame, step.leftOn) === null}
        describedAs="The field of these rows that is matched"
        placeholder="Search the fields"
        offers={(typed) => [
          counted(
            'Fields',
            fieldOffers(
              frame,
              typed,
              anyField((leftOn) => {
                onChange({ ...step, leftOn });
              }),
            ),
          ),
        ]}
      />
      <span data-tone="compare" data-look="compare" className="dg-block dg-fixed">
        ==
      </span>
      <Slot
        label={step.rightOn}
        tone="table"
        look="plain"
        describedAs={`The field of ${step.table} that is matched`}
        placeholder="Search the fields"
        offers={(typed) => [
          counted(
            `Fields of ${step.table}`,
            (table?.frame ?? [])
              .filter((column) => matches(typed, column.name))
              .map((column) =>
                offer(
                  column.name,
                  'table',
                  column.dtype,
                  () => {
                    onChange({ ...step, rightOn: column.name });
                  },
                  {
                    reason:
                      leftDtype === undefined || keysFit(column.dtype, leftDtype)
                        ? null
                        : 'a different dtype',
                  },
                ),
              ),
          ),
        ]}
      />
      <Word apart>how</Word>
      <Slot
        label={step.how}
        tone="value"
        look="plain"
        describedAs="Which rows the join keeps"
        placeholder="Search"
        offers={(typed) => [
          titled(
            'Keep',
            HOWS.filter((how) => matches(typed, how)).map((how) =>
              offer(
                how,
                'value',
                how === 'left' ? 'every row, matched or not' : 'only the rows that match',
                () => {
                  onChange({ ...step, how });
                },
              ),
            ),
          ),
        ]}
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
    first: boolean,
    current: string,
    describedAs: string,
    update: (name: string) => PivotStep,
  ): React.JSX.Element => (
    <>
      <Word apart={!first}>{word}</Word>
      <Slot
        label={current}
        tone="column"
        look="plain"
        wrong={findColumn(frame, current) === null}
        describedAs={describedAs}
        placeholder="Search the fields"
        offers={(typed) => [
          counted(
            'Fields',
            fieldOffers(
              frame,
              typed,
              anyField((name) => {
                onChange(update(name));
              }),
            ),
          ),
        ]}
      />
    </>
  );
  const values = distinctValues(frame, step.on);
  return (
    <>
      {field('on', true, step.on, 'The field whose values become columns', (on) => ({
        ...step,
        on,
        // Pinned columns are values of the old field, so pin the new field's instead.
        pinned: step.columnsFrom === 'pinned' ? distinctValues(frame, on) : [],
      }))}
      {field('index', false, step.index, 'The field that stays as rows', (index) => ({
        ...step,
        index,
      }))}
      {field('values', false, step.values, 'The field the cells are filled from', (chosen) => ({
        ...step,
        values: chosen,
      }))}
      <Word apart>aggregate</Word>
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
          <Word apart>columns</Word>
          <ManySlot
            entries={step.pinned.map((value) => ({
              id: cellLabel(value),
              blocks: [{ label: cellLabel(value), tone: 'value', look: 'plain' }],
            }))}
            empty="no values"
            describedAs={`The values of ${step.on} that become columns`}
            placeholder="Search the values"
            offers={(typed) => [
              counted(
                `Values of ${step.on}`,
                values
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
        <Hint
          lines={[
            result.added.join(', '),
            fromData
              ? 'These come from the data. Here they come from the sample rows, so a full run may return others.'
              : 'Only the pinned values become columns.',
          ]}
          code={false}
        >
          <span className="dg-dtype">{String(result.frame.length)} columns</span>
        </Hint>
      )}
    </>
  );
}

/** How many values of a new column its hover note shows. */
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
    const dtype = checkChain(step.expr, frame).dtypes.at(-1);
    if (dtype === undefined) return null;
    return (
      <Hint lines={peek(result, result.added[0] ?? '')} code>
        <span className="dg-dtype">{dtype}</span>
      </Hint>
    );
  }
  if (step.verb === 'join') {
    const [only] = result.added;
    return (
      <Hint
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
      </Hint>
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
  const { ref, handleRef, isDragSource } = useSortable({
    id: step.id,
    index,
    group: STEP_GROUP,
    type: 'step',
    accept: 'step',
  });
  const [adding, setAdding] = useState<HTMLElement | null>(null);
  const [root, setRoot] = useState<HTMLElement | null>(null);
  useLineNumbers(root, String(index + 1));
  const closeList = useCallback(() => {
    setAdding(null);
  }, []);
  const broken = stepBreak(step);
  const edge = result.problem === null ? 'var(--color-accent)' : 'var(--color-wrong)';
  return (
    <div
      ref={(element) => {
        setRoot(element);
        ref(element);
      }}
      className={cx(
        'dg-ground group/step relative border-b border-line bg-panel first:rounded-t-lg last:rounded-b-lg last:border-b-0',
        VERB_COLOURS[step.verb],
        selected && 'bg-row [--ground:var(--color-row)]',
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
      <ListOwner outletClassName="ml-[60px]">
        {(setRow) => (
          <div ref={setRow} className="flex items-start gap-1.5 py-2 pr-3 pl-1">
            <button
              ref={handleRef}
              type="button"
              title="Drag to reorder"
              aria-label={`Step ${String(index + 1)}: drag to reorder`}
              className="inline-flex h-6 w-7 shrink-0 cursor-grab touch-none items-center justify-center rounded-sm font-mono text-[11px] text-muted select-none hover:bg-hover hover:text-ink active:cursor-grabbing"
            >
              <span className="group-hover/step:hidden">{index + 1}</span>
              <span aria-hidden="true" className="hidden text-sm group-hover/step:inline">
                ⠿
              </span>
            </button>
            <div data-dg-flow className="dg-flow flex-1">
              <Piece
                tone="verb"
                bracketed
                remove={{ label: `Remove step ${String(index + 1)}`, onPress: onRemove }}
                add={{ label: `Add a step after step ${String(index + 1)}`, onPress: setAdding }}
              >
                <Hint lines={[stepText(step)]} code>
                  <Slot
                    label={VERB_LABELS[step.verb]}
                    tone="verb"
                    look="verb"
                    describedAs={`Step ${String(index + 1)}: ${VERB_LABELS[step.verb]}. Change what the step does`}
                    placeholder="Search the steps"
                    offers={(typed) =>
                      verbOffers(typed, (verb) => {
                        if (verb !== step.verb) {
                          onChange({ ...defaultStep(verb, frame), id: step.id });
                        }
                      })
                    }
                  />
                </Hint>
                <Head step={step} frame={frame} onChange={onChange} />
              </Piece>
              {step.verb === 'with_columns' && (
                <>
                  <Equals />
                  <ChainBlocks
                    chainId={step.id}
                    chain={step.expr}
                    frame={frame}
                    opensNew={false}
                    onChange={(chain) => {
                      onChange({ ...step, expr: { ...chain, alias: step.expr.alias } });
                    }}
                  />
                </>
              )}
              <span data-dg-flow-end className="ml-auto inline-flex items-center gap-2 pl-3">
                <Outcome step={step} frame={frame} result={result} onChange={onChange} />
              </span>
            </div>
          </div>
        )}
      </ListOwner>
      {result.problem !== null && (
        <p className="m-0 pb-2 pl-[42px] text-[13px] text-wrong">{result.problem}</p>
      )}
      {adding !== null && (
        <Popover anchor={adding} onDismiss={closeList}>
          <OfferList
            placeholder="Search the steps"
            groups={(typed) => verbOffers(typed, onInsertAfter)}
            keepOpen={false}
            wide={false}
            onDone={closeList}
          />
        </Popover>
      )}
    </div>
  );
}
