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

import type { Look, Offer, OfferGroup, Tone } from '@dageist/design';
import { Hint, OfferList, Piece, Popover, Seam, Slot, Word } from '@dageist/design';

import { useCatalogue } from './CatalogueProvider';
import { FreshContext, Line, Lines } from './Lists';
import { OptionRows, optionOffers } from './Parameters';
import { ConditionBlocks, ValueSlot, emptyTest } from './Values';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { Catalogue } from './model/catalogue';
import type { ConditionWord } from './model/catalogue';
import {
  conditionEntry,
  conditionId,
  elementEntry,
  methodEntry,
  methodId,
} from './model/catalogue';
import type { ArgumentRule, Method, MethodName } from './model/expr';
import {
  acceptanceMisfit,
  argumentMisfit,
  argumentRule,
  defaultArgument,
  literalComparand,
  methodOptionParameters,
  previewedMethods,
  readComparand,
  skippedOptions,
  swappedArgument,
} from './model/expr';
import type { Dtype, Frame } from './model/frame';
import { findColumn } from './model/frame';
import { nextId } from './model/ids';
import { NO_OPTIONS, carriedOptions } from './model/settings';
import type { Arm, Chain, ChainCheck, ElementScope, Source, Value } from './model/source';
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
 * Lists the methods the catalogue offers and the storyboard can preview that
 * can be handed `input`, under the catalogue's families from the shortest to
 * the longest. A method that does not fit says what it needs.
 */
function methodOffers(
  catalogue: Catalogue,
  typed: string,
  input: Dtype | null,
  onPick: (name: MethodName) => void,
): OfferGroup[] {
  const methods = previewedMethods(catalogue);
  const groups = catalogue.view.families
    .map((family) =>
      counted(
        family.title,
        methods
          .filter(
            ({ entry }) => entry.family === family.key && matches(typed, entry.name, entry.hint),
          )
          .map(({ name, entry }) => {
            const misfit = input === null ? null : acceptanceMisfit(catalogue, name, input);
            return offer(
              entry.name,
              'function',
              entry.hint,
              () => {
                onPick(name);
              },
              { reason: input === null ? 'fix the marked part first' : (misfit?.short ?? null) },
            );
          }),
      ),
    )
    .filter((group) => group.offers.length > 0);
  groups.sort((left, right) => left.offers.length - right.offers.length);
  return groups;
}

/** Returns a condition with nothing picked yet, and the value it gives. */
function emptyArm(): Arm {
  return { id: nextId('arm'), condition: emptyTest(), returns: { kind: 'unset' } };
}

interface ArgumentSlotProps {
  readonly method: Method;
  /** What the catalogue says the method's argument is. */
  readonly rule: ArgumentRule;
  readonly frame: Frame;
  /** The dtype the method is handed, when it is known. */
  readonly input: Dtype | null;
  /** Whether the chain's problem is at this method. */
  readonly wrong: boolean;
  readonly onCommit: (argument: string) => void;
}

/**
 * Draws an argument that takes an expression: a field, or a value typed in.
 * It opens the list it is picked from, which offers the values and the fields
 * whose dtype the parameter takes. Every method's argument of this kind is
 * drawn here.
 */
function ArgumentSlot({
  method,
  rule,
  frame,
  input,
  wrong,
  onCommit,
}: ArgumentSlotProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const comparand = readComparand(method.argument);
  const column = comparand.kind === 'column' ? findColumn(frame, comparand.column) : null;
  const rows = frame[0]?.cells.length ?? 0;
  let shown: { label: string; tone: Tone; look: Look };
  if (comparand.kind === 'unset') {
    shown = { label: rule.parameter.shown, tone: 'value', look: 'none' };
  } else if (comparand.kind === 'column') {
    shown = { label: comparand.column, tone: 'column', look: 'plain' };
  } else if (typeof comparand.value === 'number') {
    shown = { label: String(comparand.value), tone: 'value', look: 'plain' };
  } else {
    shown = { label: comparand.value, tone: 'value', look: 'quoted' };
  }
  // Why a value or a field does not go with what the method is handed, or `null` when it does.
  const reasonFor = (given: Parameters<typeof argumentMisfit>[3]): string | null =>
    input === null ? null : (argumentMisfit(catalogue, rule, input, given, rows)?.short ?? null);
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
          { id: 'number', reason: reasonFor({ kind: 'number', value: Number(text) }) },
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
          { id: 'text', look: 'quoted', reason: reasonFor({ kind: 'text', value: typed }) },
        ),
      );
    }
    return [
      titled('Value', written),
      counted(
        'Fields',
        fieldOffers(frame, typed, {
          ...anyField(onCommit),
          reasonFor: (dtype) => reasonFor({ kind: 'field', name: '', dtype }),
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
        describedAs={`${rule.shown}: ${rule.parameter.label}`}
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
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const entry = methodEntry(catalogue, method.name);
  const rule = argumentRule(catalogue, method.name);
  // What the catalogue calls the method; a method it lacks keeps its Polars name.
  const shown = entry?.name ?? method.name;
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
  const swap = (name: MethodName): void => {
    const next = methodEntry(catalogue, name);
    replace({
      id: method.id,
      name,
      argument: swappedArgument(catalogue, method, name, frame),
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
                    optionOffers(parameters, method.options, typed, (options, added) => {
                      replace({ ...method, options });
                      if (added !== null) fresh.mark(`${method.id}:${added}`);
                    }),
                  ]),
              ...methodOffers(catalogue, typed, input, swap),
            ]}
            buttonRef={handleRef}
          />
        </Hint>
        {rule !== null && (
          <ArgumentSlot
            method={method}
            rule={rule}
            frame={frame}
            input={input}
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
          around={{ frame, items: () => [] }}
          skipped={skippedOptions(catalogue, method)}
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
  /** The frame the chain is worked out on. */
  readonly frame: Frame;
  /** Where `element` is meant, when the chain is worked out for the values of one pivot cell. */
  readonly element?: ElementScope | undefined;
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
  element,
  onRemove,
  onChange,
}: ChainBlocksProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const check = checkChain(catalogue, chain, frame, element ?? null);
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
    const method = {
      id: nextId('method'),
      name,
      argument: defaultArgument(catalogue, name),
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
  const startsWrong = check.problem?.index === -1;
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
                openNow={fresh.id === chainId}
                buttonRef={setStart}
                describedAs="What the chain starts from"
                placeholder="Search the fields and functions"
                offers={(typed) => [
                  titled(
                    'Functions',
                    matches(typed, whenWord.text, whenWord.hint)
                      ? [
                          offer(
                            whenWord.text,
                            'function',
                            whenWord.hint,
                            () => {
                              const made = emptyArm();
                              withSource({
                                kind: 'when',
                                arms: [made],
                                otherwise: { kind: 'unset' },
                              });
                              fresh.mark(made.condition.id);
                            },
                            { reason: whenWord.lacking ? 'not in the catalogue' : null },
                          ),
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
              tone="function"
              onChange={(condition) => {
                withSource({ ...source, arms: [{ ...arm, condition }] });
              }}
            />
          </Line>
          <Line>
            <Word title={thenWord.hint}>{thenWord.text}</Word>
            {given(arm.returns, 'The value where the condition holds', (returns) => {
              withSource({ ...source, arms: [{ ...arm, returns }] });
            })}
          </Line>
          <Line>
            <Word title={otherwiseWord.hint}>{otherwiseWord.text}</Word>
            {given(source.otherwise, 'The value where the condition does not hold', (otherwise) => {
              withSource({ ...source, otherwise });
            })}
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
              methodOffers(catalogue, typed, check.dtypes[adding.at] ?? null, (name) => {
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
