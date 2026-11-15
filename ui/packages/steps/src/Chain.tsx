/**
 * @file A chain: what it starts from, then its methods, each behind a caret,
 * written out as a line of code. A chain starts from a field or from the
 * function `when`, which is set out on lines of its own: its condition, then
 * the values it gives after the words `then` and `otherwise`, and the chain's
 * methods carrying on from it on a last line. The word that names a thing,
 * a method's name or the chain's start, opens the list it is swapped from
 * and ends that list with adding a method after it and taking it away. A
 * method's name also drags it to another place in the chain. Where a chain
 * is a value, taking away what it starts from takes the whole chain away.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { PointerActivationConstraints, PointerSensor, StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable, useSortable } from '@dnd-kit/react/sortable';
import { createContext, useCallback, useContext, useState } from 'react';

import type { Look, Offer, OfferGroup, Tone } from '@dageist/design';
import { Hint, OfferList, Piece, Popover, Seam, Slot, TypedBlock, Word } from '@dageist/design';

import { FreshContext, Line, Lines } from './Lists';
import { ConditionBlocks, ValueSlot, emptyTest } from './Values';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { Method, MethodName } from './model/expr';
import {
  ARGUMENT_KINDS,
  METHOD_GROUPS,
  METHOD_GROUP_ORDER,
  METHOD_GROUP_TITLES,
  METHOD_HINTS,
  METHOD_NAMES,
  defaultArgument,
  literalComparand,
  methodDtype,
  methodRequirement,
  operandFits,
  readComparand,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';
import { findColumn, isNumeric } from './model/frame';
import { nextId } from './model/ids';
import type { Arm, Chain, ChainCheck, Source, Value } from './model/source';
import { checkChain, dtypesAgree } from './model/source';

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

/** Returns the field `source` is most about: its own, or the one a `when` gives. */
function columnBehind(source: Source): string {
  if (source.kind === 'column') return source.column;
  const returned = source.arms.map((arm) => arm.returns).find((value) => value.kind === 'column');
  return returned?.kind === 'column' ? returned.column : '';
}

/** Returns a condition with nothing picked yet, and the value it gives. */
function emptyArm(): Arm {
  return { id: nextId('arm'), condition: emptyTest(), returns: { kind: 'unset' } };
}

interface ComparandSlotProps {
  readonly method: Method;
  readonly frame: Frame;
  /** The dtype the comparison is handed, when it is known. */
  readonly input: Dtype | null;
  /** Whether the chain's problem is at this method. */
  readonly wrong: boolean;
  readonly onCommit: (argument: string) => void;
}

/**
 * Draws what a comparison is given: a field, a number or text. It opens the
 * list it is picked from, where a value can be typed.
 */
function ComparandSlot({
  method,
  frame,
  input,
  wrong,
  onCommit,
}: ComparandSlotProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const comparand = readComparand(method.argument);
  const column = comparand.kind === 'column' ? findColumn(frame, comparand.column) : null;
  let shown: { label: string; tone: Tone; look: Look };
  if (comparand.kind === 'unset') {
    shown = { label: 'value', tone: 'value', look: 'none' };
  } else if (comparand.kind === 'column') {
    shown = { label: comparand.column, tone: 'column', look: 'plain' };
  } else if (typeof comparand.value === 'number') {
    shown = { label: String(comparand.value), tone: 'value', look: 'plain' };
  } else {
    shown = { label: comparand.value, tone: 'value', look: 'quoted' };
  }
  const offers = (typed: string): OfferGroup[] => {
    const written: Offer[] = [];
    const text = typed.trim();
    if (WHOLE_NUMBER.test(text) && Number.isSafeInteger(Number(text))) {
      written.push(
        offer(
          text,
          'value',
          'a number',
          () => {
            onCommit(literalComparand(Number(text)));
          },
          {
            id: 'number',
            reason: input !== null && !isNumeric(input) ? 'a different dtype' : null,
          },
        ),
      );
    }
    if (typed !== '') {
      written.push(
        offer(
          typed,
          'value',
          'String',
          () => {
            onCommit(literalComparand(typed));
          },
          {
            id: 'text',
            look: 'quoted',
            reason: input !== null && input !== 'String' ? 'a different dtype' : null,
          },
        ),
      );
    }
    return [
      titled('Value', written),
      counted(
        'Fields',
        fieldOffers(frame, typed, {
          ...anyField(onCommit),
          reasonFor: (dtype) =>
            input === null || dtypesAgree(input, dtype) ? null : 'a different dtype',
        }),
      ),
    ];
  };
  return (
    <Hint lines={fieldHint(column)} code={false}>
      <Slot
        label={shown.label}
        tone={shown.tone}
        look={shown.look}
        wrong={wrong || comparand.kind === 'unset'}
        openNow={fresh.id === method.id}
        describedAs={`What ${method.name} compares with`}
        placeholder="Type a value, or search the fields"
        offers={offers}
      />
    </Hint>
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
  const fieldHint = useFieldHint();
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
      <Seam />
      <Piece
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
            names
            describedAs={`Method ${method.name}: swap it, or drag it to another place`}
            placeholder="Search the methods"
            offers={(typed) => methodOffers(typed, input, swap)}
            buttonRef={handleRef}
          />
        </Hint>
        {(kind === 'operand' || kind === 'column') && (
          <Hint lines={fieldHint(field)} code={false}>
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
        {kind === 'comparand' && (
          <ComparandSlot
            method={method}
            frame={frame}
            input={input}
            wrong={wrong}
            onCommit={commit}
          />
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
  /**
   * Takes the whole chain away, where it stands as a value and something else
   * could stand there. `null` where there must be a chain, as in a step: its
   * start cannot then be removed, and a `when` gives way to a field.
   */
  readonly onRemove: (() => void) | null;
  readonly onChange: (chain: Chain) => void;
}

/**
 * Draws `chain` as what it starts from and its methods. The parts sit
 * directly in their row, so a long chain breaks before a caret and never
 * drops to a new line as a whole.
 */
export function ChainBlocks({
  chainId,
  chain,
  frame,
  opensNew,
  onRemove,
  onChange,
}: ChainBlocksProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const check = checkChain(chain, frame);
  const [insertion, setInsertion] = useState<Insertion | null>(null);
  // A chain made by carrying a field on opens its list of methods as soon as it is shown.
  const [opensMethods, setOpensMethods] = useState(() => fresh.id === `${chainId}:method`);
  const closeList = useCallback(() => {
    setInsertion(null);
    setOpensMethods(false);
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
      setOpensMethods(false);
      setInsertion({ at, anchor });
    };
  };
  const startsWrong = check.problem?.index === -1;
  // A chain is taken away by the word it starts from.
  const removeChain = onRemove === null ? null : { label: 'Remove this chain', onPress: onRemove };
  const column = source.kind === 'column' ? findColumn(frame, source.column) : null;
  const unset = source.kind === 'column' && source.column === '';
  // The word the chain starts from, which that first list of methods opens against.
  const [start, setStart] = useState<HTMLElement | null>(null);
  const adding: Insertion | null =
    insertion ??
    (opensMethods && start !== null ? { at: chain.methods.length, anchor: start } : null);

  // Each chain sorts its own methods, so a method cannot leave its chain.
  const methods = (
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
  );
  const given = (
    value: Value,
    label: string,
    onValue: (next: Value) => void,
  ): React.JSX.Element => (
    <ValueSlot
      value={value}
      frame={frame}
      label={label}
      lists={false}
      expected={null}
      onChange={onValue}
    />
  );
  const [arm] = source.kind === 'when' ? source.arms : [];

  return (
    <>
      {source.kind === 'column' && (
        <>
          <Piece
            remove={removeChain}
            add={{
              label: unset ? 'Add a method after the field' : `Add a method after ${source.column}`,
              onPress: addAt(0),
            }}
          >
            <Hint lines={fieldHint(column)} code={false}>
              <Slot
                label={unset ? 'field' : source.column}
                tone={unset ? 'value' : 'column'}
                look={unset ? 'none' : 'plain'}
                wrong={startsWrong}
                names
                openNow={opensNew && fresh.id === chainId}
                buttonRef={setStart}
                describedAs="What the chain starts from"
                placeholder="Search the fields and functions"
                offers={(typed) => [
                  titled(
                    'Functions',
                    matches(typed, 'when', 'one value or another, by a condition')
                      ? [
                          offer('when', 'function', 'one value or another, by a condition', () => {
                            const made = emptyArm();
                            withSource({
                              kind: 'when',
                              arms: [made],
                              otherwise: { kind: 'unset' },
                            });
                            fresh.mark(made.condition.id);
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
          </Piece>
          {methods}
        </>
      )}
      {source.kind === 'when' && arm !== undefined && (
        // A when is set out on lines: its condition, what it gives, and the chain carrying on.
        <Lines>
          <Line>
            <Piece
              remove={
                removeChain ?? {
                  label: 'Remove when: start the chain from a field',
                  onPress: () => {
                    withSource({ kind: 'column', column: columnBehind(source) });
                  },
                }
              }
              add={{ label: 'Add a method after when', onPress: addAt(0) }}
            >
              <Slot
                label="when"
                tone="function"
                look="plain"
                names
                describedAs="The function the chain starts from"
                placeholder="Search the functions"
                offers={(typed) => [
                  titled(
                    'Functions',
                    matches(typed, 'when', 'one value or another, by a condition')
                      ? [
                          offer(
                            'when',
                            'function',
                            'one value or another, by a condition',
                            () => undefined,
                          ),
                        ]
                      : [],
                  ),
                ]}
              />
            </Piece>
            <ConditionBlocks
              condition={arm.condition}
              frame={frame}
              tone="function"
              onChange={(condition) => {
                withSource({ ...source, arms: [{ ...arm, condition }] });
              }}
            />
          </Line>
          <Line>
            <Word>then</Word>
            {given(arm.returns, 'The value where the condition holds', (returns) => {
              withSource({ ...source, arms: [{ ...arm, returns }] });
            })}
          </Line>
          <Line>
            <Word>otherwise</Word>
            {given(source.otherwise, 'The value where the condition does not hold', (otherwise) => {
              withSource({ ...source, otherwise });
            })}
          </Line>
          {chain.methods.length > 0 && <Line>{methods}</Line>}
        </Lines>
      )}
      {adding !== null && (
        <Popover anchor={adding.anchor} onDismiss={closeList}>
          <OfferList
            placeholder="Search the methods"
            groups={(typed) =>
              methodOffers(typed, check.dtypes[adding.at] ?? null, (name) => {
                insert(adding.at, name);
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
