/**
 * @file Edits a chain: what it starts from, then its method parts. A chain
 * can start from a column or from `when … then … otherwise`, and a value
 * inside a condition can be a chain of its own, so the pieces here use each
 * other. A condition and a sub-chain are drawn as collapsible sub-lists
 * under the row they belong to.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable } from '@dnd-kit/react/sortable';
import { createContext, useCallback, useContext, useState } from 'react';

import type { Choice } from '@dageist/design';
import { CommitField, Hint, Picker, Popover, cx } from '@dageist/design';

import { inputLines } from './InputsStrip';
import { MethodMenu } from './MethodMenu';
import { MethodPart, Seam, withMethodInserted, withMethodMoved } from './Parts';
import { Expandable, SubRow } from './SubList';
import { columnChoices } from './choices';
import type { Cell, Dtype, Frame } from './model/frame';
import { distinctValues, isNumeric } from './model/frame';
import { nextId } from './model/ids';
import { INPUTS, findInput } from './model/inputs';
import type { Arm, Chain, Condition, Source, Value, WhenSource } from './model/source';
import { COMPARISONS, checkChain, literalText } from './model/source';

/** How every drag-and-drop area in the editor is set up. */
export interface DragSettings {
  readonly plugins?: (defaults: Plugins) => Plugins;
}

/** The drag-and-drop settings the editor's chains share. */
export const DragSettingsContext = createContext<DragSettings>({});

const WHOLE_NUMBER = /^-?\d+$/;

function Keyword({ children }: { readonly children: string }): React.JSX.Element {
  return <span className="px-1 font-mono text-[12.5px] text-operator">{children}</span>;
}

/** Returns the column `source` is most about: its own, or the first one a condition returns. */
function columnBehind(source: Source, frame: Frame): string {
  if (source.kind === 'column') return source.column;
  const returned = source.arms.map((arm) => arm.returns).find((value) => value.kind === 'column');
  return returned?.kind === 'column' ? returned.column : (frame[0]?.name ?? '');
}

/** Returns a new arm that tests the frame's first column against its first value. */
function defaultArm(frame: Frame, returns: Value): Arm {
  const first = frame[0]?.name ?? '';
  return {
    id: nextId('arm'),
    condition: {
      column: first,
      comparison: '==',
      value: { kind: 'literal', value: distinctValues(frame, first)[0] ?? null },
    },
    returns,
  };
}

/**
 * Returns a condition to start a chain from in place of `source`: it returns
 * the column the chain started from when the condition is true.
 */
function conditionOn(source: Source, frame: Frame, dtype: Dtype | null): Source {
  const column = columnBehind(source, frame);
  // A number falls back to 0 and anything else to None, so both sides agree in dtype.
  const fallback = dtype !== null && isNumeric(dtype) ? 0 : null;
  return {
    kind: 'when',
    arms: [defaultArm(frame, { kind: 'column', column })],
    otherwise: { kind: 'literal', value: fallback },
  };
}

/** Lists the kinds a value can be changed to. */
function kindChoices(value: Value, frame: Frame, onChange: (value: Value) => void): Choice[] {
  const column = value.kind === 'column' ? value.column : (frame[0]?.name ?? '');
  const literal = value.kind === 'literal' ? value.value : undefined;
  const offer = (label: string, current: boolean, next: Value): Choice => ({
    label,
    current,
    reason: null,
    onPick: () => {
      onChange(next);
    },
  });
  return [
    offer('a text value', typeof literal === 'string', { kind: 'literal', value: '' }),
    offer('a number', typeof literal === 'number', { kind: 'literal', value: 0 }),
    offer('None', literal === null, { kind: 'literal', value: null }),
    offer('a column', value.kind === 'column', { kind: 'column', column }),
    offer('a subchain', value.kind === 'chain', {
      kind: 'chain',
      id: nextId('chain'),
      chain: { source: { kind: 'column', column }, methods: [] },
    }),
  ];
}

/** What a value slot shows and reports. */
interface ValueSlotProps {
  readonly value: Value;
  readonly frame: Frame;
  /** Literals worth offering by name, such as the values the compared column holds. */
  readonly suggestions: readonly Cell[];
  /** What the value is for, for assistive technology. */
  readonly label: string;
  /** Whether the slot takes a connected list, as `is_in` does, instead of a single value. */
  readonly lists: boolean;
  readonly onChange: (value: Value) => void;
}

/**
 * Draws a value. A column is a picker. A literal is a field with a small
 * marker in front that says what kind it is, so text needs no quotes typed
 * around it. A subchain is a collapsible row of its own.
 */
function ValueSlot({
  value,
  frame,
  suggestions,
  label,
  lists,
  onChange,
}: ValueSlotProps): React.JSX.Element {
  const kinds = kindChoices(value, frame, onChange);
  // The node's connections of the kind this slot takes: single values, or lists.
  const connected: Choice[] = INPUTS.filter(
    (input) => input.kind === (lists ? 'list' : 'value'),
  ).map((input) => ({
    label: `${input.name} · ${lists ? 'list of ' : ''}${'dtype' in input ? input.dtype : ''} · input`,
    current: value.kind === 'input' && value.name === input.name,
    reason: null,
    onPick: () => {
      onChange({ kind: 'input', name: input.name });
    },
  }));
  const literals: Choice[] = suggestions.map((suggestion) => ({
    label: literalText(suggestion),
    current: value.kind === 'literal' && value.value === suggestion,
    reason: null,
    onPick: () => {
      onChange({ kind: 'literal', value: suggestion });
    },
  }));
  const marker = (text: string): React.JSX.Element => (
    <Picker
      label={text}
      tone="plain"
      className="px-1 text-[10px] tracking-wide text-muted"
      choices={[...literals, ...connected, ...kinds]}
    />
  );

  if (lists || value.kind === 'input') {
    const input = value.kind === 'input' ? findInput(value.name) : null;
    return (
      <Hint lines={input === null ? [] : inputLines(input)} code={false}>
        <Picker
          label={value.kind === 'input' ? value.name : 'choose a list'}
          tone="input"
          choices={lists ? connected : [...connected, ...literals, ...kinds]}
        />
      </Hint>
    );
  }
  if (value.kind === 'chain') {
    return (
      <Expandable label="subchain" className="text-ink">
        <SubRow>
          {marker('subchain')}
          <ChainEditor
            chainId={value.id}
            chain={value.chain}
            frame={frame}
            onChange={(chain) => {
              onChange({ kind: 'chain', id: value.id, chain });
            }}
          />
        </SubRow>
      </Expandable>
    );
  }
  if (value.kind === 'column') {
    return (
      <Picker
        label={value.column}
        tone="column"
        choices={[
          ...columnChoices(frame, value.column, (column) => {
            onChange({ kind: 'column', column });
          }),
          ...connected,
          ...literals,
          ...kinds,
        ]}
      />
    );
  }
  if (value.value === null) {
    return <Picker label="None" tone="term" choices={[...literals, ...connected, ...kinds]} />;
  }
  if (typeof value.value === 'number') {
    return (
      <span className="inline-flex items-center">
        {marker('123')}
        <CommitField
          value={String(value.value)}
          label={label}
          numeric
          code
          accepts={(typed) => WHOLE_NUMBER.test(typed.trim())}
          onCommit={(typed) => {
            onChange({ kind: 'literal', value: Number(typed.trim()) });
          }}
        />
      </span>
    );
  }
  return (
    <span className="inline-flex items-center">
      {marker('abc')}
      <span aria-hidden="true" className="font-mono text-[12.5px] text-muted">
        &quot;
      </span>
      <CommitField
        value={value.value}
        label={label}
        numeric={false}
        code
        onCommit={(typed) => {
          onChange({ kind: 'literal', value: typed });
        }}
      />
      <span aria-hidden="true" className="font-mono text-[12.5px] text-muted">
        &quot;
      </span>
    </span>
  );
}

/** What the condition slots show and report. */
export interface ConditionSlotsProps {
  readonly condition: Condition;
  readonly frame: Frame;
  readonly onChange: (condition: Condition) => void;
}

/** Draws `condition` as a column, a comparison and a value. */
export function ConditionSlots({
  condition,
  frame,
  onChange,
}: ConditionSlotsProps): React.JSX.Element {
  return (
    <>
      <Picker
        label={condition.column}
        tone="column"
        choices={columnChoices(frame, condition.column, (column) => {
          // The old value belonged to the old column, so start from one of the new column's.
          const first = distinctValues(frame, column)[0] ?? null;
          onChange({ ...condition, column, value: { kind: 'literal', value: first } });
        })}
      />
      <Picker
        label={condition.comparison}
        tone="operator"
        choices={COMPARISONS.map((comparison) => ({
          label: comparison,
          current: comparison === condition.comparison,
          reason: null,
          onPick: () => {
            const wasList = condition.comparison === 'is_in';
            if (comparison === 'is_in' && !wasList) {
              // Membership needs a list, so start from the first connected one.
              const list = INPUTS.find((input) => input.kind === 'list');
              onChange({
                ...condition,
                comparison,
                value: { kind: 'input', name: list?.name ?? '' },
              });
            } else if (comparison !== 'is_in' && wasList) {
              const first = distinctValues(frame, condition.column)[0] ?? null;
              onChange({ ...condition, comparison, value: { kind: 'literal', value: first } });
            } else {
              onChange({ ...condition, comparison });
            }
          },
        }))}
      />
      <ValueSlot
        value={condition.value}
        frame={frame}
        suggestions={distinctValues(frame, condition.column)}
        label={`Value compared with ${condition.column}`}
        lists={condition.comparison === 'is_in'}
        onChange={(value) => {
          onChange({ ...condition, value });
        }}
      />
    </>
  );
}

interface WhenListProps {
  readonly source: WhenSource;
  readonly frame: Frame;
  readonly onChange: (source: WhenSource) => void;
  /** Goes back to starting the chain from a column. */
  readonly onUseColumn: () => void;
}

/** Draws `when … then … otherwise` as a collapsible sub-list: one row per condition. */
function WhenList({ source, frame, onChange, onUseColumn }: WhenListProps): React.JSX.Element {
  const withArm = (next: Arm): void => {
    onChange({ ...source, arms: source.arms.map((arm) => (arm.id === next.id ? next : arm)) });
  };
  return (
    <Expandable label="subchain" className="text-ink">
      {source.arms.map((arm, position) => (
        <SubRow key={arm.id}>
          <Picker
            label="when"
            tone="operator"
            className="w-[76px] justify-start font-normal"
            choices={[
              {
                label: '✕ Remove this condition',
                current: false,
                reason: source.arms.length === 1 ? 'the only condition' : null,
                onPick: () => {
                  onChange({
                    ...source,
                    arms: source.arms.filter((existing) => existing.id !== arm.id),
                  });
                },
              },
              {
                label: 'Start from a column instead',
                current: false,
                reason: null,
                onPick: onUseColumn,
              },
            ]}
          />
          <ConditionSlots
            condition={arm.condition}
            frame={frame}
            onChange={(condition) => {
              withArm({ ...arm, condition });
            }}
          />
          <Keyword>then</Keyword>
          <ValueSlot
            value={arm.returns}
            frame={frame}
            suggestions={[]}
            label={`Value returned by condition ${String(position + 1)}`}
            lists={false}
            onChange={(returns) => {
              withArm({ ...arm, returns });
            }}
          />
        </SubRow>
      ))}
      <SubRow>
        <span className="inline-flex w-[76px]">
          <Keyword>otherwise</Keyword>
        </span>
        <ValueSlot
          value={source.otherwise}
          frame={frame}
          suggestions={[]}
          label="Value returned when no condition is true"
          lists={false}
          onChange={(otherwise) => {
            onChange({ ...source, otherwise });
          }}
        />
        <button
          type="button"
          className="ml-2 cursor-pointer rounded-md px-1.5 text-xs leading-[22px] text-muted opacity-0 group-focus-within/sub:opacity-100 group-hover/sub:opacity-100 hover:bg-hover hover:text-accent"
          onClick={() => {
            onChange({ ...source, arms: [...source.arms, defaultArm(frame, source.otherwise)] });
          }}
        >
          + when
        </button>
      </SubRow>
    </Expandable>
  );
}

/** Where a method is about to be added: its position in the chain and the seam it opens from. */
interface Insertion {
  readonly at: number;
  readonly anchor: HTMLElement;
}

/** What a chain editor shows and reports. */
export interface ChainEditorProps {
  /** Identifies the chain; its parts can only be dragged within it. */
  readonly chainId: string;
  readonly chain: Chain;
  /** The frame the chain is worked out on. */
  readonly frame: Frame;
  readonly onChange: (chain: Chain) => void;
}

/** Draws `chain` as what it starts from followed by its method parts. */
export function ChainEditor({
  chainId,
  chain,
  frame,
  onChange,
}: ChainEditorProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const check = checkChain(chain, frame);
  const [insertion, setInsertion] = useState<Insertion | null>(null);
  const closeMenu = useCallback(() => {
    setInsertion(null);
  }, []);
  const withSource = (source: Source): void => {
    onChange({ source, methods: chain.methods });
  };
  const seam = (at: number): React.JSX.Element => (
    <Seam
      dtype={check.dtypes[at] ?? null}
      last={at === chain.methods.length}
      label={at === chain.methods.length ? 'Add a method' : 'Insert a method here'}
      onOpen={(anchor) => {
        setInsertion({ at, anchor });
      }}
    />
  );
  const { source } = chain;

  return (
    // The chain's pieces sit directly in their row, so a long chain wraps
    // piece by piece instead of dropping to a new line as a whole.
    <span className="contents">
      <span
        className={cx(
          'inline-flex flex-wrap items-center gap-y-1 rounded-md',
          check.problem?.index === -1 && 'bg-invalid outline-1 outline-danger',
        )}
      >
        {source.kind === 'column' ? (
          <Picker
            label={source.column}
            tone="column"
            choices={[
              ...columnChoices(frame, source.column, (column) => {
                withSource({ kind: 'column', column });
              }),
              {
                label: 'when … then … otherwise',
                current: false,
                reason: null,
                onPick: () => {
                  withSource(conditionOn(source, frame, check.dtypes[0] ?? null));
                },
              },
            ]}
          />
        ) : (
          <WhenList
            source={source}
            frame={frame}
            onChange={withSource}
            onUseColumn={() => {
              withSource({ kind: 'column', column: columnBehind(source, frame) });
            }}
          />
        )}
      </span>
      {/* Each chain sorts its own parts, so a part cannot leave its chain. */}
      <DragDropProvider
        {...settings}
        onDragEnd={(event) => {
          const dragged = event.operation.source;
          if (event.canceled || !isSortable(dragged)) return;
          if (dragged.index !== dragged.initialIndex) {
            onChange(withMethodMoved(chain, dragged.initialIndex, dragged.index));
          }
        }}
      >
        {chain.methods.map((method, index) => (
          <MethodPart
            key={method.id}
            seam={seam(index)}
            chainId={chainId}
            index={index}
            method={method}
            expr={chain}
            check={check}
            frame={frame}
            onChange={onChange}
          />
        ))}
      </DragDropProvider>
      {seam(chain.methods.length)}
      {insertion !== null && (
        <Popover anchor={insertion.anchor} onDismiss={closeMenu}>
          <MethodMenu
            input={check.dtypes[insertion.at] ?? null}
            frame={frame}
            onPick={(method) => {
              onChange(withMethodInserted(chain, insertion.at, method));
              setInsertion(null);
            }}
          />
        </Popover>
      )}
    </span>
  );
}
