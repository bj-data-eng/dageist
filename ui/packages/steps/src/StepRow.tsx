/**
 * @file One step of the list, written as a line of code: its number, its name, what it takes, and
 * at the end of the row what it returns. Nothing about the row changes when it is pointed at. The
 * step's name is the word that names it: its list changes what the step does, and ends with adding
 * a step after it and removing it. The number drags the step to another place. What a step takes
 * that is more than one thing is set out on lines beneath it. What is drawn is the step the
 * document holds, and each change is sent as an edit.
 */

import { useSortable } from '@dnd-kit/react/sortable';
import { useCallback, useContext, useMemo, useState } from 'react';

import type { OfferGroup } from '@dageist/design';
import type {
  ArgumentView,
  CallView,
  DocumentStepView,
  EntryView,
  FrameRequest,
} from '@dageist/host';
import {
  Block,
  Hint,
  OfferList,
  Peek,
  Piece,
  Popover,
  Slot,
  TypedBlock,
  Word,
  cx,
} from '@dageist/design';

import { StepProvider, useListEdits, useStepAt, useStepEdits } from './editor';
import { useCatalogue, useNode } from './EngineProvider';
import { listedGroups } from './engineLists';
import { FreshContext } from './Lists';
import type { Surroundings } from './Parameters';
import {
  OptionRows,
  ParameterDrawing,
  TableDrawing,
  optionOffers,
  useOptionToggle,
} from './Parameters';
import { ConditionBlocks, ValueSlot } from './Values';
import { matches, offer, titled } from './offers';
import type { Asked } from './ask';
import { useOptions, useStepStarts } from './ask';
import { useListState } from './positions';
import { stepColour, stepHead, stepNote } from './verbs';
import type { Catalogue, HeadName } from './model/catalogue';
import {
  HEAD,
  VERBS,
  finishingEntries,
  groupedHeadOf,
  havingEntry,
  optionParameters,
  positionOfParameter,
  stepOptionParameters,
  verbId,
  verbOf,
} from './model/catalogue';
import type { Path } from './model/doc';
import {
  NO_PATH,
  expressionOf,
  givenOf,
  keySegment,
  linkSegment,
  parameterSegment,
  shapeOf,
  under,
  valueSegment,
} from './model/doc';
import { groupLayoutOf } from './model/group';
import type { Frame } from './model/frame';
import { findTable, tablesOf } from './model/inputs';
import { chosenOptions } from './model/offers';
import type { PreparedStep } from './model/report';
import { rowEndOf, sayingOf } from './model/report';
import { NO_POSITIONS, within } from './model/place';
import { fieldArgument, listArgument } from './model/requests';
import { actionsOf } from './model/words';
import { PreparedStepProvider } from './engine';

/** The sortable group every step row belongs to. */
export const STEP_GROUP = 'steps';

/**
 * Says why the step of `entry` cannot be picked yet, or `null` when it can: the engine has not said
 * what it starts with, or has none to say.
 */
function startReason(
  catalogue: Catalogue,
  entry: string,
  starts: Asked<ReadonlySet<string>>,
): string | null {
  // A step the editor has and the catalogue lacks cannot be chosen.
  if (!catalogue.entries.has(entry)) return 'not in the catalogue';
  switch (starts.state) {
    case 'failed':
      return starts.reason;
    case 'ready':
      return starts.answer.has(entry) ? null : 'the engine gives this step no start';
    default:
      return 'waiting for the engine';
  }
}

/**
 * Lists the steps that can be added or changed to, in the words of the catalogue. A step is picked
 * with what the engine says it starts with, so it waits for the engine's answer.
 */
export function stepOffers(
  catalogue: Catalogue,
  typed: string,
  starts: Asked<ReadonlySet<string>>,
  onPick: (entry: string) => void,
): OfferGroup[] {
  return [
    titled(
      'Step',
      VERBS.map((verb) => verbId(verb))
        .filter((entry) => matches(typed, stepHead(catalogue, entry), stepNote(catalogue, entry)))
        .map((entry) =>
          offer(
            stepHead(catalogue, entry),
            'ink',
            stepNote(catalogue, entry),
            () => {
              if (starts.state === 'ready' && starts.answer.has(entry)) onPick(entry);
            },
            { look: 'verb', reason: startReason(catalogue, entry, starts) },
          ),
        ),
    ),
  ];
}

/** Returns what the engine needs to know to start a step on `frame`: its columns, and the tables to bring in. */
export function frameRequest(
  frame: Frame,
  tables: ReadonlyArray<{ readonly name: string }>,
): FrameRequest {
  return { columns: frame.map((column) => column.name), tables: tables.map((table) => table.name) };
}

/**
 * Returns the items a parameter that follows the parameter `of` holds one value for each of: the
 * keys of a sort that are given, each by the name of its field or else its place in the list.
 */
function keyLabels(call: CallView, of: string): readonly string[] {
  const keys = givenOf(call, of);
  if (keys?.kind !== 'many') return [];
  return keys.items.flatMap((item, index) => {
    const expression = expressionOf(item);
    const shape = expression === null ? null : shapeOf(expression);
    if (shape === null || shape.kind === 'unset') return [];
    return [shape.kind === 'column' ? shape.name : `key ${String(index + 1)}`];
  });
}

interface HeadProps {
  readonly call: CallView;
  readonly frame: Frame;
}

/** Draws the parameter `name` of the step by its kind and shape, as the catalogue gives them. */
function StepParameter({
  call,
  name,
  frame,
}: HeadProps & { readonly name: HeadName }): React.JSX.Element {
  const catalogue = useCatalogue();
  const parameter = catalogue.entries
    .get(call.entry)
    ?.parameters.find((candidate) => candidate.name === name);
  if (parameter === undefined) {
    return (
      <span className="font-sans text-xs text-wrong">
        {`The catalogue has no parameter ${name} of ${call.entry}.`}
      </span>
    );
  }
  return (
    <ParameterDrawing
      parameter={parameter}
      argument={givenOf(call, name)}
      path={[parameterSegment(name)]}
      around={{ frame, items: (of) => keyLabels(call, of) }}
    />
  );
}

/** What a named chain is called in a step, for the words that describe it. */
interface NamedWords {
  /** What its name names: the new column, or the key. */
  readonly named: string;
  /** What the chain holds, said of `name`, which is empty where it is not typed yet. */
  readonly holds: (name: string) => string;
}

/**
 * Draws one named chain of a step: its name, and what it holds. An `Add column` step holds its new
 * columns so, and a step that groups holds its named keys and its named aggregates so.
 */
function NamedChain({
  call,
  callPath,
  parameter,
  words,
  frame,
  at,
  alias,
  value,
  fresh,
}: HeadProps & {
  /** The way to the call that holds the named chains. */
  readonly callPath: Path;
  /** The parameter of the call that holds them. */
  readonly parameter: string;
  readonly words: NamedWords;
  readonly at: number;
  readonly alias: ArgumentView | undefined;
  readonly value: ArgumentView | undefined;
  readonly fresh: boolean;
}): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const name = alias?.kind === 'columns' ? (alias.names[0] ?? '') : '';
  // What the chain holds is an item of what the step selects.
  const holds = positionOfParameter(catalogue.entries.get(call.entry) ?? null, parameter);
  const namedPath = under(callPath, parameterSegment(parameter));
  const valuePath = under(namedPath, valueSegment(at));
  return (
    <>
      <TypedBlock
        value={name}
        kind="name"
        label={`Name of the ${words.named}`}
        wrong={name.trim() === ''}
        focusNow={fresh}
        accepts={(typed) => typed.trim() !== ''}
        onCommit={(typed) => {
          void edits.setArgument(under(namedPath, keySegment(at)), {
            kind: 'columns',
            names: [typed.trim()],
          });
        }}
      />
      <Word>=</Word>
      <ValueSlot
        expression={expressionOf(value)}
        path={valuePath}
        frame={frame}
        label={words.holds(name)}
        positions={within(NO_POSITIONS, holds)}
        argument={null}
        onSet={(request) => {
          void edits.setArgument(valuePath, request);
        }}
      />
    </>
  );
}

/** The words of the new columns of an `Add column` step. */
const NEW_COLUMN: NamedWords = {
  named: 'new column',
  holds: (name) => `What ${name === '' ? 'the column' : name} holds`,
};

function AddColumnHead({ call, frame }: HeadProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const step = useStepAt().id;
  const named = givenOf(call, HEAD.with_columns.named);
  const entries = named?.kind === 'pairs' ? named.entries : [];
  const [first, ...more] = entries;
  return (
    <>
      {first === undefined ? (
        // No new column is in the document: the list of them is drawn with what adds one.
        <StepParameter call={call} name={HEAD.with_columns.named} frame={frame} />
      ) : (
        <NamedChain
          call={call}
          callPath={NO_PATH}
          parameter={HEAD.with_columns.named}
          words={NEW_COLUMN}
          frame={frame}
          at={0}
          alias={first.key}
          value={first.value}
          fresh={fresh.id === step}
        />
      )}
      {more.map((entry, at) => (
        <span key={at + 1} className="dg-line">
          <NamedChain
            call={call}
            callPath={NO_PATH}
            parameter={HEAD.with_columns.named}
            words={NEW_COLUMN}
            frame={frame}
            at={at + 1}
            alias={entry.key}
            value={entry.value}
            fresh={false}
          />
        </span>
      ))}
      {givenOf(call, HEAD.with_columns.exprs) !== undefined && (
        <StepParameter call={call} name={HEAD.with_columns.exprs} frame={frame} />
      )}
    </>
  );
}

function SortHead({ call, frame }: HeadProps): React.JSX.Element {
  return (
    <>
      <Word>by</Word>
      <StepParameter call={call} name={HEAD.sort.by} frame={frame} />
    </>
  );
}

function FilterHead({ call, frame }: HeadProps): React.JSX.Element {
  const catalogue = useCatalogue();
  return (
    <ConditionBlocks
      path={[parameterSegment(HEAD.filter.predicates)]}
      argument={givenOf(call, HEAD.filter.predicates)}
      frame={frame}
      positions={within(
        NO_POSITIONS,
        positionOfParameter(catalogue.entries.get(call.entry) ?? null, HEAD.filter.predicates),
      )}
      tone="verb"
    />
  );
}

function JoinHead({ call, frame }: HeadProps): React.JSX.Element {
  const node = useNode();
  const edits = useStepEdits();
  const other = givenOf(call, HEAD.join.other);
  const name = other?.kind === 'table' ? other.name : null;
  const table = name === null ? null : findTable(node, name);
  return (
    <>
      <TableDrawing
        describedAs="The table that is joined"
        value={name}
        onChange={(picked) => {
          const joined = findTable(node, picked);
          const first = joined?.columns[0]?.name;
          // The keys belonged to the old table, so start from the new table's first field: two
          // edits, the second made only if the first was.
          void edits.together((make) => {
            const picking = make.setArgument([parameterSegment(HEAD.join.other)], {
              kind: 'table',
              name: picked,
            });
            return first === undefined
              ? [picking]
              : [
                  picking,
                  make.setArgument(
                    [parameterSegment(HEAD.join.rightOn)],
                    listArgument([fieldArgument(first)]),
                  ),
                ];
          });
        }}
      />
      <Word>on</Word>
      <StepParameter call={call} name={HEAD.join.leftOn} frame={frame} />
      <span data-tone="compare" data-look="compare" className="dg-block dg-fixed">
        ==
      </span>
      <StepParameter call={call} name={HEAD.join.rightOn} frame={table?.columns ?? []} />
    </>
  );
}

function PivotHead({ call, frame }: HeadProps): React.JSX.Element {
  return (
    <>
      <Word>on</Word>
      <StepParameter call={call} name={HEAD.pivot.on} frame={frame} />
      <Word>index</Word>
      <StepParameter call={call} name={HEAD.pivot.index} frame={frame} />
      <Word>values</Word>
      <StepParameter call={call} name={HEAD.pivot.values} frame={frame} />
      <Word>aggregate_function</Word>
      <StepParameter call={call} name={HEAD.pivot.aggregate} frame={frame} />
    </>
  );
}

/** The words of the named keys of a step that groups. */
const NAMED_KEY: NamedWords = {
  named: 'key',
  holds: (name) => `What the key ${name === '' ? '' : `${name} `}holds`,
};

/** The parameters of an `agg` that hold its aggregates, and those that are named. */
const AGGS = 'aggs';
const NAMED_AGGS = 'named_aggs';

/** The words of the named aggregates of an `agg`. */
const NAMED_AGGREGATE: NamedWords = {
  named: 'column',
  holds: (name) => `What ${name === '' ? 'the column' : name} aggregates`,
};

/** Draws the named chains of the parameter `parameter` of the call at `callPath`, or what adds the first. */
function NamedChains({
  call,
  callPath,
  parameter,
  words,
  frame,
}: HeadProps & {
  readonly callPath: Path;
  readonly parameter: string;
  readonly words: NamedWords;
}): React.JSX.Element {
  const named = givenOf(call, parameter);
  const entries = named?.kind === 'pairs' ? named.entries : [];
  const catalogue = useCatalogue();
  const found = catalogue.entries
    .get(call.entry)
    ?.parameters.find((candidate) => candidate.name === parameter);
  return (
    <>
      {entries.map((entry, at) => (
        <span key={at} className="dg-line">
          <NamedChain
            call={call}
            callPath={callPath}
            parameter={parameter}
            words={words}
            frame={frame}
            at={at}
            alias={entry.key}
            value={entry.value}
            fresh={false}
          />
        </span>
      ))}
      {entries.length === 0 && found !== undefined && (
        // None is in the document: the list of them is drawn with what adds one.
        <ParameterDrawing
          parameter={found}
          argument={named}
          path={under(callPath, parameterSegment(parameter))}
          around={{ frame, items: () => [] }}
        />
      )}
    </>
  );
}

/** The parameter of a `having` that holds its predicates. */
const PREDICATES = 'predicates';

/** Draws the predicate of a `having` that keeps the groups it holds, and what takes it away. */
function HavingLine({ call, at, frame }: HeadProps & { readonly at: number }): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const actions = actionsOf('step');
  const callPath = [linkSegment(at)];
  return (
    <span className="dg-line">
      <Piece
        remove={
          actions.remove
            ? {
                label: 'Remove the having',
                onPress: () => {
                  void edits.removeGroupCall(callPath);
                },
              }
            : null
        }
        add={null}
      >
        <Word>having</Word>
      </Piece>
      <ConditionBlocks
        path={under(callPath, parameterSegment(PREDICATES))}
        argument={givenOf(call, PREDICATES)}
        frame={frame}
        positions={within(
          NO_POSITIONS,
          positionOfParameter(catalogue.entries.get(call.entry) ?? null, PREDICATES),
        )}
        tone="verb"
      />
    </span>
  );
}

/** Draws the call that finishes a grouping: its name, which a list changes, and what it takes. */
function FinishingLine({
  call,
  at,
  frame,
}: HeadProps & { readonly at: number }): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const list = useListState();
  const callPath = [linkSegment(at)];
  const toggle = useOptionToggle(callPath);
  const entry = catalogue.entries.get(call.entry) ?? null;
  const optionsAsked = useOptions(
    list.open && entry !== null
      ? { entry: entry.id, chosen: chosenOptions(entry, call.arguments) }
      : null,
  );
  // The parameters of the call that a list can add, which the call itself draws once they are set.
  const parameters = entry === null ? [] : optionParameters(entry, groupedHeadOf(entry.id));
  const given = new Set(call.arguments.map((held) => held.parameter));
  const around = { frame, items: () => [] };
  return (
    <span className="dg-line">
      <Hint lines={entry === null ? [] : [entry.hint]} code={false}>
        <Slot
          label={entry?.name ?? call.entry}
          tone="verb"
          look="verb"
          names
          describedAs={`The call that finishes the grouping: ${entry?.name ?? call.entry}. Change what it does`}
          placeholder="Search the calls"
          offers={(typed) =>
            listedGroups(
              { word: 'step', options: parameters.length > 0 },
              {
                options: () => [optionOffers(parameters, given, typed, toggle, optionsAsked)],
                steps: () => [
                  titled(
                    'Finish with',
                    finishingEntries(catalogue)
                      .filter((candidate) => matches(typed, candidate.name, candidate.hint))
                      .map((candidate) =>
                        offer(candidate.name, 'ink', candidate.hint, () => {
                          if (candidate.id !== call.entry) {
                            void edits.swapGroupCall(callPath, candidate.id);
                          }
                        }),
                      ),
                  ),
                ],
              },
            )
          }
          onOpenChange={list.onOpenChange}
        />
      </Hint>
      {entry !== null && (
        <>
          {groupedHeadOf(entry.id).includes(AGGS) && (
            <>
              <ParameterOf entry={entry} call={call} name={AGGS} path={callPath} around={around} />
              <NamedChains
                call={call}
                callPath={callPath}
                parameter={NAMED_AGGS}
                words={NAMED_AGGREGATE}
                frame={frame}
              />
            </>
          )}
        </>
      )}
      <OptionRows
        entry={entry}
        call={call}
        callPath={callPath}
        parameters={parameters}
        around={around}
        layout="inline"
      />
    </span>
  );
}

/** Draws the parameter `name` of `call` by its kind. */
function ParameterOf({
  entry,
  call,
  name,
  path,
  around,
}: {
  readonly entry: EntryView;
  readonly call: CallView;
  readonly name: string;
  readonly path: Path;
  readonly around: Surroundings;
}): React.JSX.Element {
  const parameter = entry.parameters.find((candidate) => candidate.name === name);
  if (parameter === undefined) return <></>;
  return (
    <ParameterDrawing
      parameter={parameter}
      argument={givenOf(call, name)}
      path={under(path, parameterSegment(name))}
      around={around}
    />
  );
}

/** Draws a step that groups: its keys, the predicate that keeps some groups, and the call that finishes it. */
function GroupHead({
  step,
  frame,
}: {
  readonly step: DocumentStepView;
  readonly frame: Frame;
}): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const { call } = step;
  const layout = groupLayoutOf(catalogue, step);
  const having = havingEntry(catalogue);
  return (
    <>
      <Word>by</Word>
      <StepParameter call={call} name={HEAD.group_by.by} frame={frame} />
      <NamedChains
        call={call}
        callPath={NO_PATH}
        parameter={HEAD.group_by.named}
        words={NAMED_KEY}
        frame={frame}
      />
      {layout.having === null && having !== null && (
        <span className="dg-line">
          <Block
            label="+ having"
            tone="ink"
            look="plain"
            waits
            describedAs="Keep only the groups that a predicate keeps"
            onPress={() => {
              // The having stands before the call that finishes the grouping, which is the last.
              void edits.insertGroupCall(0, having.id);
            }}
          />
        </span>
      )}
      {layout.having !== null && (
        <HavingLine call={layout.having.call} at={layout.having.at} frame={frame} />
      )}
      {layout.finishing !== null && (
        <FinishingLine call={layout.finishing.call} at={layout.finishing.at} frame={frame} />
      )}
    </>
  );
}

/** Draws what a step takes, by the step it is. */
function Head({
  step,
  frame,
}: {
  readonly step: DocumentStepView;
  readonly frame: Frame;
}): React.JSX.Element | null {
  const { call } = step;
  const verb = verbOf(call.entry);
  switch (verb) {
    case 'with_columns':
      return <AddColumnHead call={call} frame={frame} />;
    case 'filter':
      return <FilterHead call={call} frame={frame} />;
    case 'sort':
      return <SortHead call={call} frame={frame} />;
    case 'group_by':
      return <GroupHead step={step} frame={frame} />;
    case 'select':
      return <StepParameter call={call} name={HEAD.select.exprs} frame={frame} />;
    case 'rename':
      return <StepParameter call={call} name={HEAD.rename.mapping} frame={frame} />;
    case 'join':
      return <JoinHead call={call} frame={frame} />;
    case 'pivot':
      return <PivotHead call={call} frame={frame} />;
    case null:
      return null;
    default:
      return verb satisfies never;
  }
}

/** What the band above a step says when the engine collects the rows so far there. */
const COLLECT_NOTE = 'The lazy query stops here. The rows so far are loaded into memory.';

interface OutcomeProps {
  readonly call: CallView;
  readonly result: PreparedStep;
}

/** Draws what stands at the end of a step's row: the dtype it returns, what it adds, or the rows it keeps. */
function Outcome({ call, result }: OutcomeProps): React.JSX.Element | null {
  const end = rowEndOf(call, result);
  if (end === null) return null;
  switch (end.kind) {
    case 'pivot':
      return (
        <Peek
          describedAs="The columns the pivot returns"
          lines={[
            end.added.join(', '),
            'These are the values of the connected table that give its columns.',
          ]}
          code={false}
        >
          <span className="dg-dtype">{String(end.columns)} columns</span>
        </Peek>
      );
    case 'column':
      // The first values come from the rows the engine ran; without them the dtype is all there is.
      if (end.first === null) return <span className="dg-dtype">{end.dtype}</span>;
      return (
        <Peek
          describedAs={`${end.alias} is ${end.dtype}: show its first values`}
          lines={[end.first]}
          code
        >
          <span className="dg-dtype">{end.dtype}</span>
        </Peek>
      );
    case 'join': {
      const [only] = end.added;
      return (
        <Peek
          describedAs="The columns the join adds: show their first values"
          lines={end.added.flatMap((column) => [
            `${column.name} · ${column.dtype}`,
            ...(column.first === null ? [] : [column.first]),
          ])}
          code
        >
          <span className="text-xs text-muted">
            adds{' '}
            <span className="dg-dtype">
              {end.added.length === 1 && only !== undefined
                ? only.name
                : `${String(end.added.length)} columns`}
            </span>
          </span>
        </Peek>
      );
    }
    case 'kept':
      return (
        <span className="text-xs text-muted">
          keeps{' '}
          <span className="dg-dtype">{`${String(end.kept)} of ${String(end.received)} rows`}</span>
        </span>
      );
    default:
      return end satisfies never;
  }
}

/** What a step row shows and reports. */
export interface StepRowProps {
  readonly step: DocumentStepView;
  readonly index: number;
  /** The frame this step is given. */
  readonly frame: Frame;
  readonly result: PreparedStep;
  readonly selected: boolean;
  readonly onSelect: () => void;
  /** Adds a step of `entry` as the `at`th, started on `frame`. */
  readonly onAdd: (at: number, entry: string, frame: FrameRequest) => void;
}

/** Draws one step as a row that can be edited, selected, dragged and removed. */
export function StepRow({
  step,
  index,
  frame,
  result,
  selected,
  onSelect,
  onAdd,
}: StepRowProps): React.JSX.Element {
  const at = useMemo(() => ({ index, id: step.id }), [index, step.id]);
  return (
    <StepProvider value={at}>
      <PreparedStepProvider value={result}>
        <Row
          step={step}
          index={index}
          frame={frame}
          result={result}
          selected={selected}
          onSelect={onSelect}
          onAdd={onAdd}
        />
      </PreparedStepProvider>
    </StepProvider>
  );
}

function Row({
  step,
  index,
  frame,
  result,
  selected,
  onSelect,
  onAdd,
}: StepRowProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const node = useNode();
  const list = useListState();
  const listEdits = useListEdits();
  const toggle = useOptionToggle(NO_PATH);
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
  const number = String(index + 1);
  const { call } = step;
  const name = stepHead(catalogue, call.entry);
  const entry = catalogue.entries.get(call.entry) ?? null;
  const missing = entry === null;
  // What the step's options may be set to, asked of the engine while the step's list is open.
  const optionsAsked = useOptions(
    list.open && entry !== null
      ? {
          entry: entry.id,
          chosen: chosenOptions(entry, call.arguments),
        }
      : null,
  );
  // What each step starts with, asked of the engine while the list of steps is open, to change
  // this step into another, or to add one after it.
  const changeStarts = useStepStarts(list.open, frame, node);
  const addStarts = useStepStarts(adding !== null, result.after, node);
  const parameters = stepOptionParameters(catalogue, call.entry);
  const given = new Set(call.arguments.map((held) => held.parameter));
  const tables = tablesOf(node);
  const saying = sayingOf(result);
  const actions = actionsOf('step');
  return (
    <div
      ref={ref}
      data-step-id={step.id}
      className={cx(
        'dg-ground border-b border-line bg-panel first:rounded-t-lg last:rounded-b-lg last:border-b-0',
        stepColour(call.entry),
        selected && 'bg-row [--ground:var(--color-row)]',
        isDragSource && 'opacity-60',
      )}
      onPointerDownCapture={onSelect}
      onFocusCapture={onSelect}
    >
      {!result.lazy && (
        <div className="flex items-center gap-2 border-b border-dashed border-operator px-3 py-0.5 text-[11px] text-muted">
          <span className="rounded-sm bg-invalid px-1.5 font-mono font-semibold text-operator">
            collect
          </span>
          {COLLECT_NOTE}
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
            remove={
              actions.remove
                ? {
                    label: `Remove ${name}`,
                    onPress: () => {
                      void listEdits.removeStep({ index, id: step.id });
                    },
                  }
                : null
            }
            add={actions.add ? { label: `Add a step after ${name}`, onPress: setAdding } : null}
          >
            <Hint lines={result.summary === '' ? [] : [result.summary]} code={false}>
              <Slot
                label={name}
                tone="verb"
                look="verb"
                names
                describedAs={`Step ${number}: ${name}. Change what the step does`}
                placeholder="Search the steps"
                offers={(typed) =>
                  // The parameters the step can be given, then the steps it can be changed to.
                  listedGroups(
                    { word: 'step', options: parameters.length > 0 },
                    {
                      options: () => [optionOffers(parameters, given, typed, toggle, optionsAsked)],
                      steps: () =>
                        stepOffers(catalogue, typed, changeStarts, (picked) => {
                          if (picked !== call.entry) {
                            void listEdits.swapStep(
                              { index, id: step.id },
                              picked,
                              frameRequest(frame, tables),
                            );
                          }
                        }),
                    },
                  )
                }
                onOpenChange={list.onOpenChange}
              />
            </Hint>
          </Piece>
          <Head step={step} frame={frame} />
          <OptionRows
            entry={entry}
            call={call}
            callPath={NO_PATH}
            parameters={parameters}
            around={{ frame, items: (of) => keyLabels(call, of) }}
            layout="lines"
          />
          {missing && (
            <span className="dg-line font-sans text-xs text-wrong">
              {`The catalogue has no entry ${call.entry}, so this step has no words to show.`}
            </span>
          )}
          {saying.says === 'problem' &&
            saying.lines.map((line) => (
              <span key={line} className="dg-line font-sans text-xs text-wrong">
                {line}
              </span>
            ))}
        </div>
        <span className="shrink-0 pl-3 text-xs text-muted">
          <Outcome call={call} result={result} />
        </span>
      </div>
      {adding !== null && (
        <Popover anchor={adding} onDismiss={closeList}>
          <OfferList
            placeholder="Search the steps"
            groups={(typed) =>
              listedGroups(
                { word: 'newStep' },
                {
                  steps: () =>
                    stepOffers(catalogue, typed, addStarts, (picked) => {
                      onAdd(index + 1, picked, frameRequest(result.after, tables));
                    }),
                },
              )
            }
            keepOpen={false}
            wide={false}
            onDone={closeList}
          />
        </Popover>
      )}
    </div>
  );
}
