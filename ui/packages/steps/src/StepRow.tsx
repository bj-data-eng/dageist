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

import type { OfferGroup } from '@dageist/design';
import { Hint, OfferList, Peek, Piece, Popover, Slot, TypedBlock, Word, cx } from '@dageist/design';

import { useCatalogue, useNode } from './EngineProvider';
import { FreshContext } from './Lists';
import {
  ColumnDrawing,
  OptionRows,
  ParameterDrawing,
  TableDrawing,
  optionOffers,
} from './Parameters';
import { ConditionBlocks, ValueSlot } from './Values';
import { matches, offer, titled } from './offers';
import type { Frame } from './model/frame';
import { findColumn } from './model/frame';
import type {
  FilterStep,
  JoinStep,
  PivotStep,
  RenameStep,
  SelectStep,
  SortStep,
  Step,
  Verb,
  WithColumnsStep,
} from './model/steps';
import type { PreparedStep } from './model/report';
import { VERBS, defaultStep, fieldKeys, stepItems, stepOptionParameters } from './model/steps';
import { optionOf, starterSetting, withOption } from './model/settings';
import { chosenOptions } from './model/offers';
import { NO_POSITIONS, within } from './model/place';
import { useOptions } from './ask';
import { useListState } from './positions';
import type { Catalogue } from './model/catalogue';
import { positionOfParameter, verbEntry, verbId } from './model/catalogue';
import { findTable, tablesOf } from './model/inputs';
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
            // A verb the editor has and the catalogue lacks cannot be chosen.
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
  const catalogue = useCatalogue();
  // What the new column holds is an item of what the step selects.
  const holds = positionOfParameter(verbEntry(catalogue, 'with_columns'), 'named_exprs');
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
        positions={within(NO_POSITIONS, holds)}
        argument={null}
        onChange={(value) => {
          onChange({ ...step, value });
        }}
      />
    </>
  );
}

interface StepParameterProps {
  readonly step: Step;
  /** The Python name of the parameter, which the step holds in its options. */
  readonly name: string;
  /** The rows the parameter's fields are picked from. */
  readonly frame: Frame;
  /** Tells what was just added from the rest. */
  readonly keyPrefix: string;
  readonly onChange: (step: Step) => void;
}

/** Draws the parameter `name` of `step` by its kind and shape, as the catalogue gives them. */
function StepParameter({
  step,
  name,
  frame,
  keyPrefix,
  onChange,
}: StepParameterProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const parameter = verbEntry(catalogue, step.verb)?.parameters.find(
    (candidate) => candidate.name === name,
  );
  if (parameter === undefined) {
    return (
      <span className="font-sans text-xs text-wrong">
        {`The catalogue has no parameter ${name} of ${verbId(step.verb)}.`}
      </span>
    );
  }
  return (
    <ParameterDrawing
      parameter={parameter}
      setting={optionOf(step.options, name) ?? starterSetting(parameter)}
      around={{ frame, items: (of) => stepItems(step, of) }}
      keyPrefix={keyPrefix}
      onChange={(setting) => {
        onChange({ ...step, options: withOption(step.options, name, setting) });
      }}
    />
  );
}

function SortHead({ step, frame, onChange }: HeadProps<SortStep>): React.JSX.Element {
  return (
    <>
      <Word>by</Word>
      <StepParameter step={step} name="by" frame={frame} keyPrefix={step.id} onChange={onChange} />
    </>
  );
}

function SelectHead({ step, frame, onChange }: HeadProps<SelectStep>): React.JSX.Element {
  return (
    <StepParameter step={step} name="exprs" frame={frame} keyPrefix={step.id} onChange={onChange} />
  );
}

function RenameHead({ step, frame, onChange }: HeadProps<RenameStep>): React.JSX.Element {
  return (
    <StepParameter
      step={step}
      name="mapping"
      frame={frame}
      keyPrefix={step.id}
      onChange={onChange}
    />
  );
}

function FilterHead({ step, frame, onChange }: HeadProps<FilterStep>): React.JSX.Element {
  const catalogue = useCatalogue();
  return (
    <ConditionBlocks
      condition={step.condition}
      frame={frame}
      positions={within(
        NO_POSITIONS,
        positionOfParameter(verbEntry(catalogue, 'filter'), 'predicates'),
      )}
      tone="verb"
      onChange={(condition) => {
        onChange({ ...step, condition });
      }}
    />
  );
}

function JoinHead({ step, frame, onChange }: HeadProps<JoinStep>): React.JSX.Element {
  const node = useNode();
  const table = findTable(node, step.table);
  return (
    <>
      <TableDrawing
        describedAs="The table that is joined"
        value={step.table}
        onChange={(name) => {
          const joined = findTable(node, name);
          // The keys belonged to the old table, so start from the new table's first field.
          onChange({
            ...step,
            table: name,
            options: withOption(
              step.options,
              'right_on',
              fieldKeys([joined?.columns[0]?.name ?? '']),
            ),
          });
        }}
      />
      <Word>on</Word>
      <StepParameter
        step={step}
        name="left_on"
        frame={frame}
        keyPrefix={`${step.id}:left_on`}
        onChange={onChange}
      />
      <span data-tone="compare" data-look="compare" className="dg-block dg-fixed">
        ==
      </span>
      <StepParameter
        step={step}
        name="right_on"
        frame={table?.columns ?? []}
        keyPrefix={`${step.id}:right_on`}
        onChange={onChange}
      />
    </>
  );
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
  return (
    <>
      {field('on', step.on, 'The field whose values become columns', (on) => ({ ...step, on }))}
      {field('index', step.index, 'The field that stays as rows', (index) => ({
        ...step,
        index,
      }))}
      {field('values', step.values, 'The field the cells are filled from', (chosen) => ({
        ...step,
        values: chosen,
      }))}
      <Word>aggregate_function</Word>
      <StepParameter
        step={step}
        name="aggregate_function"
        frame={frame}
        keyPrefix={`${step.id}:aggregate_function`}
        onChange={onChange}
      />
    </>
  );
}

/** What the band above a step says when the engine collects the rows so far there. */
const COLLECT_NOTE = 'The lazy query stops here. The rows so far are loaded into memory.';

interface OutcomeProps {
  readonly step: Step;
  readonly result: PreparedStep;
}

/** Returns the names of the columns `result` says its step added. */
function addedBy(result: PreparedStep): string[] {
  return result.changes.filter((change) => change.change === 'added').map((change) => change.name);
}

/** Draws what stands at the end of a step's row: the dtype it returns, what it adds, or the rows it keeps. */
function Outcome({ step, result }: OutcomeProps): React.JSX.Element | null {
  if (result.problem !== null || result.status !== 'valid') return null;
  const dtypeOf = (name: string): string => findColumn(result.after, name)?.dtype ?? '';
  const added = addedBy(result);
  if (step.verb === 'pivot') {
    return (
      <Peek
        describedAs="The columns the pivot returns"
        lines={[
          added.join(', '),
          'These are the values of the connected table that give its columns.',
        ]}
        code={false}
      >
        <span className="dg-dtype">{String(result.after.length)} columns</span>
      </Peek>
    );
  }
  if (step.verb === 'with_columns') {
    const dtype = findColumn(result.after, step.alias)?.dtype;
    if (dtype === undefined) return null;
    const first = result.first.get(step.alias);
    // The first values come from the rows the engine ran; without them the dtype is all there is.
    if (first === undefined) return <span className="dg-dtype">{dtype}</span>;
    return (
      <Peek describedAs={`${step.alias} is ${dtype}: show its first values`} lines={[first]} code>
        <span className="dg-dtype">{dtype}</span>
      </Peek>
    );
  }
  if (step.verb === 'join') {
    const [only] = added;
    return (
      <Peek
        describedAs="The columns the join adds: show their first values"
        lines={added.flatMap((name) => {
          const first = result.first.get(name);
          return [`${name} · ${dtypeOf(name)}`, ...(first === undefined ? [] : [first])];
        })}
        code
      >
        <span className="text-xs text-muted">
          adds{' '}
          <span className="dg-dtype">
            {added.length === 1 && only !== undefined ? only : `${String(added.length)} columns`}
          </span>
        </span>
      </Peek>
    );
  }
  if (step.verb === 'filter' && result.rows !== null) {
    return (
      <span className="text-xs text-muted">
        keeps{' '}
        <span className="dg-dtype">{`${String(result.rows.kept)} of ${String(result.rows.received)} rows`}</span>
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
  readonly result: PreparedStep;
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
  const list = useListState();
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
  const name = verbHead(catalogue, step.verb);
  const entry = verbEntry(catalogue, step.verb);
  const missing = entry === null;
  // What the step's options may be set to, asked of the engine while the step's list is open.
  const optionsAsked = useOptions(
    list.open && entry !== null
      ? { entry: entry.id, chosen: chosenOptions(entry, step.options) }
      : null,
  );
  const node = useNode();
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
      {result.lazy === false && (
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
            remove={{ label: `Remove ${name}`, onPress: onRemove }}
            add={{ label: `Add a step after ${name}`, onPress: setAdding }}
          >
            <Hint lines={result.source === null ? [] : [result.source]} code>
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
                        optionOffers(
                          parameters,
                          step.options,
                          typed,
                          (options, added) => {
                            onChange({ ...step, options });
                            if (added !== null) fresh.mark(`${step.id}:${added}`);
                          },
                          optionsAsked,
                        ),
                      ]),
                  ...verbOffers(catalogue, typed, (verb) => {
                    if (verb !== step.verb) {
                      onChange({ ...defaultStep(verb, frame, tablesOf(node)), id: step.id });
                    }
                  }),
                ]}
                onOpenChange={list.onOpenChange}
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
          {result.rowsProblem !== null && (
            <span className="dg-line font-sans text-xs text-wrong">{result.rowsProblem}</span>
          )}
        </div>
        <span className="shrink-0 pl-3 text-xs text-muted">
          <Outcome step={step} result={result} />
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
