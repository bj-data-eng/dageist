/**
 * @file A chain: what it starts from, then its methods, each behind a caret,
 * written out as a line of code. A chain starts from a field or from the
 * function `when`, which is set out on lines of its own: its condition, then
 * the values it gives after the words `then` and `otherwise`, and the chain's
 * methods carrying on from it on a last line. The word that names a thing,
 * a method's name or the chain's start, opens the list it is swapped from
 * and ends that list with adding a method after it and taking it away. A
 * method's name also drags it to another place in the chain. Taking away
 * what a chain starts from takes the whole chain away.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { PointerActivationConstraints, PointerSensor, StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable, useSortable } from '@dnd-kit/react/sortable';
import { createContext, useCallback, useContext, useState } from 'react';

import type { Look, OfferGroup, Tone } from '@dageist/design';
import type { LiteralView, OfferPlaceView } from '@dageist/host';
import { Hint, OfferList, Piece, Popover, Seam, Slot, Word } from '@dageist/design';

import { useCandidateFits, useOffers, useOptions, useReadTyped } from './ask';
import type { FitSlot } from './ask';
import { useCatalogue, useNode } from './EngineProvider';
import { fieldGroup, inputGroup, methodGroups, startFunctions, valueGroup } from './engineLists';
import { inputLines } from './InputsStrip';
import { FreshContext, Line, Lines } from './Lists';
import { OptionRows, optionOffers } from './Parameters';
import { useListState, usePositions } from './positions';
import { ConditionBlocks, ValueSlot, emptyTest } from './Values';
import { matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { ConditionWord } from './model/catalogue';
import {
  conditionEntry,
  conditionId,
  elementEntry,
  methodEntry,
  methodId,
  positionOfParameter,
} from './model/catalogue';
import { wrongId } from './model/document';
import type { ArgumentRule, Method, MethodName } from './model/expr';
import {
  argumentRule,
  methodOptionParameters,
  startingArgument,
  swappedArgument,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';
import { assertNever, findColumn } from './model/frame';
import { findInput } from './model/inputs';
import { nextId } from './model/ids';
import { chosenOptions } from './model/offers';
import type { PreparedOffer } from './model/offers';
import { placeRequest, startRequest, within } from './model/place';
import { NO_OPTIONS, carriedOptions } from './model/settings';
import { literalValue, valueWords } from './model/literal';
import type { Arm, Chain, Source, Value } from './model/source';
import type { ChainDtypes } from './model/report';
import { usePreparedStep } from './engine';

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

/** No word of the editor is the matter. */
const NO_WRONG: ReadonlySet<string> = new Set();

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

/** Returns a condition with nothing picked yet, and the value it gives. */
function emptyArm(): Arm {
  return { id: nextId('arm'), condition: emptyTest(), returns: { kind: 'unset' } };
}

interface ArgumentSlotProps {
  readonly method: Method;
  /** What the catalogue says the method's argument is. */
  readonly rule: ArgumentRule;
  /** The identifier the catalogue gives the method. */
  readonly entryId: string;
  /** Where the method stands, as the engine reported it, or `null` where it did not. */
  readonly place: OfferPlaceView | null;
  readonly frame: Frame;
  /** Whether the engine refused the method. */
  readonly wrong: boolean;
  readonly onCommit: (argument: Value) => void;
}

/** Returns how an argument is shown as a word. */
function shownArgument(
  argument: Value,
  rule: ArgumentRule,
): { label: string; tone: Tone; look: Look } {
  switch (argument.kind) {
    case 'unset':
      return { label: rule.parameter.shown, tone: 'value', look: 'none' };
    case 'column':
      return { label: argument.column, tone: 'column', look: 'plain' };
    case 'input':
      return { label: argument.name, tone: 'input', look: 'plain' };
    case 'chain':
      return { label: '…', tone: 'function', look: 'plain' };
    case 'literal': {
      const words = valueWords(argument);
      if (words === null) return { label: 'None', tone: 'value', look: 'none' };
      return {
        label: words,
        tone: 'value',
        look: typeof argument.value === 'number' ? 'plain' : 'quoted',
      };
    }
    default:
      return assertNever(argument);
  }
}

/**
 * Draws an argument that takes an expression: a field, an input, or a value
 * typed in. It opens the list it is picked from, which offers what fits the
 * method's parameter, as the engine says it does. Every method's argument of
 * this kind is drawn here.
 */
function ArgumentSlot({
  method,
  rule,
  entryId,
  place,
  frame,
  wrong,
  onCommit,
}: ArgumentSlotProps): React.JSX.Element {
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const outer = usePositions();
  const list = useListState();
  const argument: Value = method.argument ?? { kind: 'unset' };
  const column = argument.kind === 'column' ? findColumn(frame, argument.column) : null;
  const shown = shownArgument(argument, rule);
  // The argument stands in the context of the chain, and is the argument of the method where
  // the engine has reported where the method stands.
  const start = within(outer, rule.parameter.position);
  const slot: FitSlot | null =
    start.length === 0 && place === null
      ? null
      : {
          start: start.length === 0 ? null : start,
          argument:
            place === null
              ? null
              : { entry: entryId, parameter: rule.parameter.name, place: placeRequest(place) },
        };
  const fits = useCandidateFits(slot, list.open, frame, node);
  const readings = useReadTyped(
    list.open ? { text: list.typed, typed: { kind: 'expression' }, slot } : null,
  );
  const offers = (typed: string): OfferGroup[] => {
    const rows = {
      frame,
      node,
      typed,
      fits,
      readings,
      inputs: ['value', 'list'] as const,
      onLiteral: (picked: LiteralView) => {
        onCommit(literalValue(picked));
      },
      onInput: (name: string) => {
        onCommit({ kind: 'input', name });
      },
      onField: (name: string) => {
        onCommit({ kind: 'column', column: name });
      },
    };
    return [valueGroup(rows), inputGroup(rows), fieldGroup(rows)];
  };
  return (
    <Hint lines={fieldHint(column)} code={false}>
      <Slot
        label={shown.label}
        tone={shown.tone}
        look={shown.look}
        wrong={wrong || argument.kind === 'unset'}
        openNow={fresh.id === method.id}
        describedAs={`${rule.shown}: ${rule.parameter.label}`}
        placeholder="Type a value, or search the fields"
        offers={offers}
        onOpenChange={list.onOpenChange}
        onType={list.onType}
      />
    </Hint>
  );
}

/** Returns the dtype a method of `chain` is handed: what stands before the method at `index`, or the start for the first. */
function dtypeBefore(dtypes: ChainDtypes | undefined, index: number): Dtype | null {
  if (dtypes === undefined) return null;
  return index === 0 ? dtypes.source : (dtypes.methods[index - 1] ?? null);
}

interface MethodLinkProps {
  /** The chain the method belongs to; it can only be dragged within it. */
  readonly chainId: string;
  readonly index: number;
  readonly method: Method;
  readonly chain: Chain;
  /** The dtypes after the chain's start and each of its methods, when the engine has reported them. */
  readonly dtypes: ChainDtypes | undefined;
  /** The words of the editor that are the matter, as `wrongId` names them. */
  readonly wrong: ReadonlySet<string>;
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
  dtypes,
  wrong: wrongWords,
  frame,
  onChange,
  onAddAfter,
}: MethodLinkProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const entry = methodEntry(catalogue, method.name);
  const rule = argumentRule(catalogue, method.name);
  // What the catalogue calls the method; a method it lacks keeps its Polars name.
  const shown = entry?.name ?? method.name;
  const input = dtypeBefore(dtypes, index);
  const output = dtypes?.methods[index] ?? null;
  // Where the method stands, as the engine reported it: what may follow what comes before it.
  const place = dtypes?.places[index] ?? null;
  const wrong = wrongWords.has(wrongId({ kind: 'method', methodId: method.id }));
  const list = useListState();
  const offered = useOffers(list.open && place !== null ? placeRequest(place) : null);
  const optionsAsked = useOptions(
    list.open && entry !== null
      ? { entry: entry.id, chosen: chosenOptions(entry, method.options) }
      : null,
  );
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
  const swap = (name: MethodName, picked: PreparedOffer): void => {
    const next = methodEntry(catalogue, name);
    replace({
      id: method.id,
      name,
      argument: swappedArgument(catalogue, method.argument, name, picked.argument),
      // What is set carries over to a method that takes the same parameter.
      options: next === null ? NO_OPTIONS : carriedOptions(method.options, entry, next),
    });
  };
  const parameters = methodOptionParameters(catalogue, method.name);
  return (
    <span ref={ref} className="dg-link">
      <Seam />
      <Piece
        dragged={isDragSource}
        remove={{
          label: `Remove ${shown}`,
          onPress: () => {
            onChange(
              withMethods(
                chain,
                chain.methods.filter((existing) => existing.id !== method.id),
              ),
            );
          },
        }}
        add={{ label: `Add a method after ${shown}`, onPress: onAddAfter }}
      >
        <Hint
          lines={[
            `${shown} · ${entry?.hint ?? 'not in the catalogue'}`,
            ...(input === null ? [] : [`${input} → ${output ?? 'does not fit'}`]),
          ]}
          code={false}
        >
          <Slot
            label={shown}
            tone="function"
            look="plain"
            wrong={wrong}
            names
            describedAs={`Method ${shown}: swap it, or drag it to another place`}
            placeholder="Search the methods"
            offers={(typed) => [
              // The parameters the method can be given, then the methods it can be swapped for.
              ...(parameters.length === 0
                ? []
                : [
                    optionOffers(
                      parameters,
                      method.options,
                      typed,
                      (options, added) => {
                        replace({ ...method, options });
                        if (added !== null) fresh.mark(`${method.id}:${added}`);
                      },
                      optionsAsked,
                    ),
                  ]),
              ...methodGroups(catalogue, offered, place !== null, typed, swap),
            ]}
            onOpenChange={list.onOpenChange}
            onType={list.onType}
            buttonRef={handleRef}
          />
        </Hint>
        {rule !== null && (
          <ArgumentSlot
            method={method}
            rule={rule}
            entryId={entry?.id ?? methodId(method.name)}
            place={place}
            frame={frame}
            wrong={wrong}
            onCommit={(argument) => {
              replace({ ...method, argument });
            }}
          />
        )}
        <OptionRows
          ownerId={method.id}
          entry={entry}
          parameters={parameters}
          options={method.options}
          around={{
            frame,
            items: () => [],
            method: entry === null ? undefined : { entry: entry.id, place },
          }}
          layout="inline"
          onChange={(options) => {
            replace({ ...method, options });
          }}
        />
        {entry === null && (
          <span className="font-sans text-xs text-wrong">{`not in the catalogue as ${methodId(method.name)}`}</span>
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
  /** The columns the chain's fields are picked from. */
  readonly frame: Frame;
  /** Takes the whole chain away, leaving its place for something else. */
  readonly onRemove: () => void;
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
  onRemove,
  onChange,
}: ChainBlocksProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const catalogue = useCatalogue();
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const prepared = usePreparedStep();
  const dtypes = prepared?.chains.get(chainId);
  const wrongWords = prepared?.wrong ?? NO_WRONG;
  const positions = usePositions();
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
  const insert = (at: number, name: MethodName, picked: PreparedOffer): void => {
    const method = {
      id: nextId('method'),
      name,
      argument: startingArgument(catalogue, name, picked.argument),
      options: NO_OPTIONS,
    };
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
  const startsWrong = wrongWords.has(wrongId({ kind: 'start', chainId }));
  // The words of a condition come from the catalogue; one it lacks keeps its Polars name and says so.
  const word = (which: ConditionWord): { text: string; hint: string; lacking: boolean } => {
    const found = conditionEntry(catalogue, which);
    return {
      text: found?.name ?? which,
      hint: found?.hint ?? 'not in the catalogue',
      lacking: found === null,
    };
  };
  const [whenWord, thenWord, otherwiseWord] = [word('when'), word('then'), word('otherwise')];
  const lacking = (['when', 'then', 'otherwise'] as const)
    .filter((which) => conditionEntry(catalogue, which) === null)
    .map((which) => conditionId(which));
  // A chain is taken away by the word it starts from.
  const removeChain = { label: 'Remove this chain', onPress: onRemove };
  const column = source.kind === 'column' ? findColumn(frame, source.column) : null;
  // What the chain starts from, when that is a field or a connected input.
  const startName =
    source.kind === 'column' ? source.column : source.kind === 'input' ? source.name : '';
  const unset = (source.kind === 'column' || source.kind === 'input') && startName === '';
  const startInput = source.kind === 'input' ? findInput(node, source.name) : null;
  // The word the chain starts from, which that first list of methods opens against.
  const [start, setStart] = useState<HTMLElement | null>(null);
  const adding: Insertion | null =
    insertion ??
    (opensMethods && start !== null ? { at: chain.methods.length, anchor: start } : null);
  // What may stand where a method would be added, as the engine says of the place it reported.
  const addPlace = adding === null ? null : (dtypes?.places[adding.at] ?? null);
  const inserting = useOffers(addPlace === null ? null : placeRequest(addPlace));
  // What may start the chain: the functions, the connected inputs and the fields the engine allows.
  const startList = useListState();
  const startKnown = positions.length > 0;
  const startOffers = useOffers(startList.open && startKnown ? startRequest(positions) : null);
  const startFits = useCandidateFits(
    startKnown ? { start: positions, argument: null } : null,
    startList.open,
    frame,
    node,
  );

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
          dtypes={dtypes}
          wrong={wrongWords}
          frame={frame}
          onChange={onChange}
          onAddAfter={addAt(index + 1)}
        />
      ))}
    </DragDropProvider>
  );
  // What a condition holds stands in a position inside the expression: its predicates in one, its
  // values in another.
  const given = (
    value: Value,
    label: string,
    which: 'then' | 'otherwise',
    onValue: (next: Value) => void,
  ): React.JSX.Element => (
    <ValueSlot
      value={value}
      frame={frame}
      label={label}
      lists={false}
      positions={within(
        positions,
        positionOfParameter(conditionEntry(catalogue, which), 'statement'),
      )}
      argument={null}
      onChange={onValue}
    />
  );
  const [arm] = source.kind === 'when' ? source.arms : [];
  // The word `element` comes from the catalogue; one it lacks keeps its Polars name and says so.
  const elementStart = elementEntry(catalogue);
  const elementWord = elementStart?.name ?? 'element';
  const elementHint = elementStart?.hint ?? 'not in the catalogue';

  return (
    <>
      {source.kind === 'element' && (
        <>
          <Piece
            remove={removeChain}
            add={{ label: `Add a method after ${elementWord}`, onPress: addAt(0) }}
          >
            <Hint lines={[`${elementWord} · ${elementHint}`]} code={false}>
              <Slot
                label={elementWord}
                tone="function"
                look="plain"
                wrong={startsWrong || elementStart === null}
                names
                buttonRef={setStart}
                describedAs="The function the chain starts from"
                placeholder="Search the functions"
                offers={(typed) => [
                  titled(
                    'Functions',
                    matches(typed, elementWord, elementHint)
                      ? [offer(elementWord, 'function', elementHint, () => undefined)]
                      : [],
                  ),
                ]}
              />
            </Hint>
          </Piece>
          {methods}
        </>
      )}
      {(source.kind === 'column' || source.kind === 'input') && (
        <>
          <Piece
            remove={removeChain}
            add={{
              label: unset ? 'Add a method after the field' : `Add a method after ${startName}`,
              onPress: addAt(0),
            }}
          >
            <Hint
              lines={startInput === null ? fieldHint(column) : inputLines(startInput)}
              code={false}
            >
              <Slot
                label={unset ? 'field' : startName}
                tone={unset ? 'value' : source.kind === 'input' ? 'input' : 'column'}
                look={unset ? 'none' : 'plain'}
                wrong={startsWrong}
                names
                openNow={fresh.id === chainId}
                buttonRef={setStart}
                describedAs="What the chain starts from"
                placeholder="Search the fields and functions"
                offers={(typed) => {
                  const rows = {
                    frame,
                    node,
                    typed,
                    fits: startFits,
                    inputs: ['value', 'list'] as const,
                    onInput: (name: string) => {
                      withSource({ kind: 'input', name });
                    },
                    onField: (name: string) => {
                      withSource({ kind: 'column', column: name });
                    },
                  };
                  return [
                    ...startFunctions(catalogue, startOffers, startKnown, typed, (id) => {
                      if (id === 'polars.when') {
                        const made = emptyArm();
                        withSource({ kind: 'when', arms: [made], otherwise: { kind: 'unset' } });
                        fresh.mark(made.condition.id);
                      } else {
                        withSource({ kind: 'element' });
                        fresh.mark(`${chainId}:method`);
                      }
                    }),
                    inputGroup(rows),
                    fieldGroup(rows),
                  ];
                }}
                onOpenChange={startList.onOpenChange}
                onType={startList.onType}
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
              remove={removeChain}
              add={{ label: `Add a method after ${whenWord.text}`, onPress: addAt(0) }}
            >
              <Hint lines={[`${whenWord.text} · ${whenWord.hint}`]} code={false}>
                <Slot
                  label={whenWord.text}
                  tone="function"
                  look="plain"
                  names
                  describedAs="The function the chain starts from"
                  placeholder="Search the functions"
                  offers={(typed) => [
                    titled(
                      'Functions',
                      matches(typed, whenWord.text, whenWord.hint)
                        ? [offer(whenWord.text, 'function', whenWord.hint, () => undefined)]
                        : [],
                    ),
                  ]}
                />
              </Hint>
            </Piece>
            <ConditionBlocks
              condition={arm.condition}
              frame={frame}
              positions={within(
                positions,
                positionOfParameter(conditionEntry(catalogue, 'when'), 'predicates'),
              )}
              tone="function"
              onChange={(condition) => {
                withSource({ ...source, arms: [{ ...arm, condition }] });
              }}
            />
          </Line>
          <Line>
            <Word title={thenWord.hint}>{thenWord.text}</Word>
            {given(arm.returns, 'The value where the condition holds', 'then', (returns) => {
              withSource({ ...source, arms: [{ ...arm, returns }] });
            })}
          </Line>
          <Line>
            <Word title={otherwiseWord.hint}>{otherwiseWord.text}</Word>
            {given(
              source.otherwise,
              'The value where the condition does not hold',
              'otherwise',
              (otherwise) => {
                withSource({ ...source, otherwise });
              },
            )}
          </Line>
          {lacking.length > 0 && (
            <Line>
              <span className="font-sans text-xs text-wrong">{`not in the catalogue as ${lacking.join(', ')}`}</span>
            </Line>
          )}
          {chain.methods.length > 0 && <Line>{methods}</Line>}
        </Lines>
      )}
      {adding !== null && (
        <Popover anchor={adding.anchor} onDismiss={closeList}>
          <OfferList
            placeholder="Search the methods"
            groups={(typed) =>
              methodGroups(catalogue, inserting, addPlace !== null, typed, (name, picked) => {
                insert(adding.at, name, picked);
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
