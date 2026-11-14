/**
 * @file A chain: what it starts from, then its methods, each behind a caret.
 * A chain starts from a field or from the function `when`, whose cases are a
 * list under the row. A method is a bracketed piece: pointing at it parts the
 * brackets, a cross takes it away and a plus puts a method after it. Its name
 * opens the list it is swapped from, and drags it to another place in the
 * chain. A faint method waits at the end of every chain.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { PointerActivationConstraints, PointerSensor, StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable, useSortable } from '@dnd-kit/react/sortable';
import { createContext, useCallback, useContext, useState } from 'react';

import type { OfferGroup } from '@dageist/design';
import { Block, Hint, OfferList, Piece, Popover, Seam, Slot, TypedBlock } from '@dageist/design';

import { Dots, FreshContext, Line } from './Lists';
import { ConditionBlocks, ValueSlot, emptyTest } from './Values';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import type { Method, MethodName } from './model/expr';
import {
  ARGUMENT_KINDS,
  METHOD_GROUPS,
  METHOD_GROUP_ORDER,
  METHOD_GROUP_TITLES,
  METHOD_HINTS,
  METHOD_NAMES,
  defaultArgument,
  methodDtype,
  methodRequirement,
  operandFits,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';
import { findColumn } from './model/frame';
import { nextId } from './model/ids';
import type { Arm, Chain, ChainCheck, Source, Value, WhenSource } from './model/source';
import { checkChain } from './model/source';

/** How every drag-and-drop area in the editor is set up. */
export interface DragSettings {
  readonly plugins?: (defaults: Plugins) => Plugins;
}

/**
 * Returns how dragging is set up for a page whose content security policy
 * accepts `styleNonce` for injected styles, or has no policy when it is
 * `null`. Dragging injects a few style rules; without a policy dnd kit's
 * defaults are right.
 */
export function dragSettingsFor(styleNonce: string | null): DragSettings {
  if (styleNonce === null) return {};
  return {
    plugins: (defaults: Plugins): Plugins => [
      ...defaults,
      StyleInjector.configure({ nonce: styleNonce }),
    ],
  };
}

/** Returns `items` with the one at `from` moved to the place `to`. */
export function moved<T>(items: readonly T[], from: number, to: number): readonly T[] {
  const next = [...items];
  const [item] = next.splice(from, 1);
  if (item === undefined) return items;
  next.splice(to, 0, item);
  return next;
}

/** The drag-and-drop settings the editor's chains share. */
export const DragSettingsContext = createContext<DragSettings>({});

/**
 * A method is dragged by its name, which is also the block that opens its
 * list. Dragging starts only after the pointer has moved, so a plain click
 * still opens the list.
 */
const PART_SENSORS = [
  PointerSensor.configure({
    activationConstraints: [new PointerActivationConstraints.Distance({ value: 6 })],
    preventActivation: () => false,
  }),
];

const WHOLE_NUMBER = /^-?\d+$/;

function withMethods(chain: Chain, methods: readonly Method[]): Chain {
  return { source: chain.source, methods };
}

function withMethodMoved(chain: Chain, from: number, to: number): Chain {
  const methods = [...chain.methods];
  const [method] = methods.splice(from, 1);
  if (method === undefined) return chain;
  methods.splice(to, 0, method);
  return withMethods(chain, methods);
}

/**
 * Lists the methods that can be handed `input`, in their groups from the
 * shortest to the longest. A method that does not fit says what it needs.
 */
function methodOffers(
  typed: string,
  input: Dtype | null,
  onPick: (name: MethodName) => void,
): OfferGroup[] {
  const groups = METHOD_GROUP_ORDER.map((group) =>
    counted(
      METHOD_GROUP_TITLES[group],
      METHOD_NAMES.filter(
        (name) => METHOD_GROUPS[name] === group && matches(typed, name, METHOD_HINTS[name]),
      ).map((name) => {
        let reason: string | null = null;
        if (input === null) reason = 'fix the marked part first';
        else if (methodDtype(name, input, null) === null) {
          reason = `needs ${methodRequirement(name)}`;
        }
        return offer(
          name,
          'function',
          METHOD_HINTS[name],
          () => {
            onPick(name);
          },
          { reason },
        );
      }),
    ),
  );
  groups.sort((left, right) => left.offers.length - right.offers.length);
  return groups;
}

/** Returns the field `source` is most about: its own, or the first one a case returns. */
function columnBehind(source: Source): string {
  if (source.kind === 'column') return source.column;
  const returned = source.arms.map((arm) => arm.returns).find((value) => value.kind === 'column');
  return returned?.kind === 'column' ? returned.column : '';
}

/** Returns a case with nothing in it yet: a comparison, then a value. */
function emptyArm(): Arm {
  return { id: nextId('arm'), condition: emptyTest(), returns: { kind: 'unset' } };
}

interface CasesProps {
  readonly source: WhenSource;
  readonly frame: Frame;
  readonly onChange: (source: WhenSource) => void;
}

/**
 * Draws the cases of a `when` as lines in columns: a condition, then a
 * value. A faint `when` waits to add a case, and `otherwise` comes last.
 */
function Cases({ source, frame, onChange }: CasesProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const withArm = (next: Arm): void => {
    onChange({ ...source, arms: source.arms.map((arm) => (arm.id === next.id ? next : arm)) });
  };
  const returned = (
    value: Value,
    label: string,
    onValue: (next: Value) => void,
  ): React.JSX.Element => (
    <ValueSlot
      value={value}
      frame={frame}
      suggestFrom={null}
      label={label}
      lists={false}
      expected={null}
      onChange={onValue}
    />
  );
  return (
    <div className="dg-cases">
      {source.arms.map((arm, position) => (
        <Line
          key={arm.id}
          waits={false}
          onRemove={
            source.arms.length === 1
              ? null
              : () => {
                  onChange({ ...source, arms: source.arms.filter((other) => other.id !== arm.id) });
                }
          }
        >
          <span className="dg-cell">
            <ConditionBlocks
              condition={arm.condition}
              frame={frame}
              tone="function"
              alone
              onChange={(condition) => {
                withArm({ ...arm, condition });
              }}
            />
          </span>
          <span className="dg-label">then</span>
          {returned(arm.returns, `Value of case ${String(position + 1)}`, (returns) => {
            withArm({ ...arm, returns });
          })}
        </Line>
      ))}
      <Line waits onRemove={null}>
        <Block
          label="when"
          tone="function"
          look="plain"
          waits
          describedAs="Add a case"
          onPress={() => {
            const arm = emptyArm();
            onChange({ ...source, arms: [...source.arms, arm] });
            fresh.mark(arm.condition.id);
          }}
        />
        <span />
        <span />
      </Line>
      <Line waits={false} onRemove={null}>
        <span className="dg-label">otherwise</span>
        <span />
        {returned(source.otherwise, 'Value when no case holds', (otherwise) => {
          onChange({ ...source, otherwise });
        })}
      </Line>
    </div>
  );
}

interface MethodLinkProps {
  /** The chain the method belongs to; it can only be dragged within it. */
  readonly chainId: string;
  readonly index: number;
  readonly method: Method;
  readonly chain: Chain;
  readonly check: ChainCheck;
  readonly frame: Frame;
  readonly onChange: (chain: Chain) => void;
  /** Opens the list of methods to put one after this one. */
  readonly onAddAfter: (piece: HTMLElement) => void;
}

/** Draws one method of a chain as a piece behind its caret. */
function MethodLink({
  chainId,
  index,
  method,
  chain,
  check,
  frame,
  onChange,
  onAddAfter,
}: MethodLinkProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const kind = ARGUMENT_KINDS[method.name];
  const argument = method.argument ?? '';
  const input = check.dtypes[index] ?? null;
  const output = check.dtypes[index + 1] ?? null;
  const wrong = check.problem?.index === index;
  const { ref, handleRef, isDragSource } = useSortable({
    id: method.id,
    index,
    group: chainId,
    type: `part:${chainId}`,
    accept: `part:${chainId}`,
    sensors: PART_SENSORS,
  });
  const replace = (next: Method): void => {
    onChange(
      withMethods(
        chain,
        chain.methods.map((existing) => (existing.id === method.id ? next : existing)),
      ),
    );
  };
  const commit = (value: string): void => {
    replace({ id: method.id, name: method.name, argument: value });
  };
  const swap = (name: MethodName): void => {
    // The argument is kept when both methods take the same kind of one.
    const keeps = ARGUMENT_KINDS[name] !== null && ARGUMENT_KINDS[name] === kind;
    replace({
      id: method.id,
      name,
      argument: keeps ? method.argument : defaultArgument(name, frame, input),
    });
  };
  const field = findColumn(frame, argument);
  return (
    <span ref={ref} className="dg-link">
      <Hint lines={input === null ? [] : [`${input} passes here`]} code={false}>
        <Seam waits={false} />
      </Hint>
      <Piece
        tone="function"
        bracketed
        dragged={isDragSource}
        remove={{
          label: `Remove ${method.name}`,
          onPress: () => {
            onChange(
              withMethods(
                chain,
                chain.methods.filter((existing) => existing.id !== method.id),
              ),
            );
          },
        }}
        add={{ label: `Add a method after ${method.name}`, onPress: onAddAfter }}
      >
        <Hint
          lines={[
            `${method.name} · ${METHOD_HINTS[method.name]}`,
            ...(input === null ? [] : [`${input} → ${output ?? 'does not fit'}`]),
          ]}
          code={false}
        >
          <Slot
            label={method.name}
            tone="function"
            look="plain"
            wrong={wrong}
            describedAs={`Method ${method.name}: swap it, or drag it to another place`}
            placeholder="Search the methods"
            offers={(typed) => methodOffers(typed, input, swap)}
            buttonRef={handleRef}
          />
        </Hint>
        {(kind === 'operand' || kind === 'column') && (
          <Hint lines={field === null ? [] : [`${field.name} · ${field.dtype}`]} code={false}>
            <Slot
              label={argument === '' ? 'field' : argument}
              tone={argument === '' ? 'value' : 'column'}
              look={argument === '' ? 'none' : 'plain'}
              wrong={wrong && argument !== ''}
              openNow={fresh.id === method.id}
              describedAs={`The field ${method.name} works with`}
              placeholder="Search the fields"
              offers={(typed) => [
                counted(
                  'Fields',
                  fieldOffers(frame, typed, {
                    ...anyField(commit),
                    reasonFor: (dtype) =>
                      kind === 'column' || input === null || operandFits(input, dtype)
                        ? null
                        : 'a different dtype',
                  }),
                ),
              ]}
            />
          </Hint>
        )}
        {(kind === 'integer' || kind === 'duration') && (
          <TypedBlock
            value={argument}
            kind={kind === 'integer' ? 'number' : 'text'}
            label={kind === 'integer' ? `How many rows ${method.name} moves by` : 'A duration'}
            wrong={wrong}
            focusNow={fresh.id === method.id}
            accepts={(typed) => kind === 'duration' || WHOLE_NUMBER.test(typed.trim())}
            onCommit={(typed) => {
              commit(typed.trim());
            }}
          />
        )}
      </Piece>
    </span>
  );
}

/** Where a method is about to be added: its place in the chain and the piece its list opens from. */
interface Insertion {
  readonly at: number;
  readonly anchor: HTMLElement;
}

/** What a chain shows and reports. */
export interface ChainBlocksProps {
  /** Identifies the chain; its methods can only be dragged within it. */
  readonly chainId: string;
  readonly chain: Chain;
  /** The frame the chain is worked out on. */
  readonly frame: Frame;
  /**
   * Whether a chain that was just added opens the list of what it starts
   * from. A step's chain does not: its name is typed first.
   */
  readonly opensNew: boolean;
  readonly onChange: (chain: Chain) => void;
}

/**
 * Draws `chain` as what it starts from, its methods and the method that
 * waits. The parts sit directly in their row, so a long chain breaks before
 * a caret and never drops to a new line as a whole.
 */
export function ChainBlocks({
  chainId,
  chain,
  frame,
  opensNew,
  onChange,
}: ChainBlocksProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const fresh = useContext(FreshContext);
  const check = checkChain(chain, frame);
  const [insertion, setInsertion] = useState<Insertion | null>(null);
  const closeList = useCallback(() => {
    setInsertion(null);
  }, []);
  const { source } = chain;
  const withSource = (next: Source): void => {
    onChange({ source: next, methods: chain.methods });
  };
  const insert = (at: number, name: MethodName): void => {
    const input = check.dtypes[at] ?? null;
    const method = { id: nextId('method'), name, argument: defaultArgument(name, frame, input) };
    const methods = [...chain.methods];
    methods.splice(at, 0, method);
    onChange(withMethods(chain, methods));
    fresh.mark(method.id);
  };
  const addAt = (at: number): ((anchor: HTMLElement) => void) => {
    return (anchor) => {
      setInsertion({ at, anchor });
    };
  };
  const startsWrong = check.problem?.index === -1;
  const column = source.kind === 'column' ? findColumn(frame, source.column) : null;
  const unset = source.kind === 'column' && source.column === '';

  return (
    <>
      {source.kind === 'column' ? (
        <Hint lines={column === null ? [] : [`${column.name} · ${column.dtype}`]} code={false}>
          <Slot
            label={unset ? 'field' : source.column}
            tone={unset ? 'value' : 'column'}
            look={unset ? 'none' : 'plain'}
            wrong={startsWrong && !unset}
            openNow={opensNew && fresh.id === chainId}
            describedAs="What the chain starts from"
            placeholder="Search the fields and functions"
            offers={(typed) => [
              titled(
                'Functions',
                matches(typed, 'when', 'one value or another, by cases')
                  ? [
                      offer('when', 'function', 'one value or another, by cases', () => {
                        const arm = emptyArm();
                        withSource({
                          kind: 'when',
                          arms: [arm],
                          otherwise: { kind: 'unset' },
                        });
                        fresh.mark(arm.condition.id);
                      }),
                    ]
                  : [],
              ),
              counted(
                'Fields',
                fieldOffers(
                  frame,
                  typed,
                  anyField((name) => {
                    withSource({ kind: 'column', column: name });
                  }),
                ),
              ),
            ]}
          />
        </Hint>
      ) : (
        <Piece
          tone="function"
          bracketed
          remove={{
            label: 'Remove when: start the chain from a field',
            onPress: () => {
              withSource({ kind: 'column', column: columnBehind(source) });
            },
          }}
          add={{ label: 'Add a method after when', onPress: addAt(0) }}
        >
          <Dots id={`${chainId}:when`} label="The cases of when" word="when">
            <Cases source={source} frame={frame} onChange={withSource} />
          </Dots>
        </Piece>
      )}
      {/* Each chain sorts its own methods, so a method cannot leave its chain. */}
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
          <MethodLink
            key={method.id}
            chainId={chainId}
            index={index}
            method={method}
            chain={chain}
            check={check}
            frame={frame}
            onChange={onChange}
            onAddAfter={addAt(index + 1)}
          />
        ))}
      </DragDropProvider>
      <span className="dg-link">
        <Seam waits />
        <Slot
          label="method"
          tone="function"
          look="plain"
          waits
          describedAs="Add a method to the end of the chain"
          placeholder="Search the methods"
          offers={(typed) =>
            methodOffers(typed, check.dtypes[chain.methods.length] ?? null, (name) => {
              insert(chain.methods.length, name);
            })
          }
        />
      </span>
      {insertion !== null && (
        <Popover anchor={insertion.anchor} onDismiss={closeList}>
          <OfferList
            placeholder="Search the methods"
            groups={(typed) =>
              methodOffers(typed, check.dtypes[insertion.at] ?? null, (name) => {
                insert(insertion.at, name);
              })
            }
            keepOpen={false}
            wide={false}
            onDone={closeList}
          />
        </Popover>
      )}
    </>
  );
}
