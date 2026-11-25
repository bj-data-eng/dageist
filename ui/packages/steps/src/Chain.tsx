/**
 * @file A chain: what it starts from, then its methods, each behind a caret, written out as a line
 * of code. A chain starts from a field, a connected input, or a function such as `when`, which is
 * set out on lines of its own: its condition, then the values it gives after the words `then` and
 * `otherwise`, and the chain's methods carrying on from it on a last line. The word that names a
 * thing, a method's name or the chain's start, opens the list it is swapped from and ends that
 * list with adding a method after it and taking it away. A method's name also drags it to another
 * place in the chain. Taking away what a chain starts from takes the whole chain away.
 *
 * What is drawn is the expression the document holds, and every pick is sent as an edit at the place
 * of what was picked, which the engine makes.
 */

import type { Plugins } from '@dnd-kit/abstract';
import { PointerActivationConstraints, PointerSensor, StyleInjector } from '@dnd-kit/dom';
import { DragDropProvider } from '@dnd-kit/react';
import { isSortable, useSortable } from '@dnd-kit/react/sortable';
import { createContext, useCallback, useContext, useState } from 'react';

import type { Look, OfferGroup, Tone } from '@dageist/design';
import type {
  ArgumentView,
  CallView,
  LiteralView,
  OfferPlaceView,
  ValueRequest,
} from '@dageist/host';
import { Hint, OfferList, Piece, Popover, Seam, Slot, Word } from '@dageist/design';

import { useCandidateFits, useOffers, useOptions, useReadTyped } from './ask';
import type { FitSlot } from './ask';
import { useStepAt, useStepEdits } from './editor';
import { useCatalogue, useNode } from './EngineProvider';
import { usePreparedStep } from './engine';
import {
  fieldGroup,
  inputGroup,
  listedGroups,
  methodGroups,
  startFunctions,
  valueGroup,
} from './engineLists';
import { FreshContext, Line, Lines } from './Lists';
import { OptionRows, freshKey, optionOffers, useOptionToggle } from './Parameters';
import { useListState, usePositions } from './positions';
import { ConditionBlocks, ValueSlot, useIsWrongAt } from './Values';
import { matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { Catalogue, ConditionWord } from './model/catalogue';
import {
  CONDITION_TESTS,
  argumentRule,
  conditionEntry,
  conditionId,
  elementEntry,
  methodId,
  methodNameOf,
  positionOfParameter,
} from './model/catalogue';
import { methodOptionParameters } from './model/controls';
import type { ExpressionView } from './model/doc';
import {
  START_SEGMENT,
  expressionOf,
  fieldNameOf,
  givenOf,
  itemSegment,
  linkSegment,
  parameterSegment,
  pathKey,
  shapeOf,
  under,
} from './model/doc';
import type { Path } from './model/doc';
import type { Frame } from './model/frame';
import { assertNever, findColumn } from './model/frame';
import { findInput, inputLines } from './model/inputs';
import type { PreparedOffer } from './model/offers';
import { chosenOptions } from './model/offers';
import { placeRequest, startRequest, within } from './model/place';
import type { ChainDtypes } from './model/report';
import { linkHint } from './model/report';
import { fieldArgument, inputArgument, listArgument, literalExpression } from './model/requests';
import { OFFERED_INPUTS, actionsOf, chainBodyOf } from './model/words';

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

/** Returns the place a call would stand at as the engine's request for it, or `null` where it is not known. */
function standingOf(
  place: OfferPlaceView | null | undefined,
): ReturnType<typeof placeRequest> | null {
  return place === undefined || place === null ? null : placeRequest(place);
}

/** Returns how an argument that is an expression, or that is not given, is shown as a word. */
function shownArgument(
  expression: ExpressionView | null,
  given: boolean,
  label: string,
): { label: string; tone: Tone; look: Look; wrong: boolean } {
  if (expression === null) {
    // An argument that is not given holds what Polars gives it, which is complete as it is.
    return { label, tone: 'value', look: 'none', wrong: given };
  }
  const shape = shapeOf(expression);
  switch (shape.kind) {
    case 'unset':
      return { label, tone: 'value', look: 'none', wrong: true };
    case 'column':
      return { label: shape.name, tone: 'column', look: 'plain', wrong: false };
    case 'input':
      return { label: shape.name, tone: 'input', look: 'plain', wrong: false };
    case 'literal':
      return {
        label: shape.shown,
        tone: 'value',
        look: shape.text ? 'quoted' : 'plain',
        wrong: false,
      };
    case 'chain':
      return { label: '…', tone: 'function', look: 'plain', wrong: false };
    default:
      return assertNever(shape);
  }
}

interface ArgumentSlotProps {
  readonly call: CallView;
  /** What the catalogue says the method's argument is. */
  readonly rule: NonNullable<ReturnType<typeof argumentRule>>;
  /** The way to the call. */
  readonly callPath: Path;
  /** Where the method stands, as the engine reported it, or `null` where it did not. */
  readonly place: OfferPlaceView | null;
  readonly frame: Frame;
  /** Whether the engine refused the method. */
  readonly wrong: boolean;
}

/**
 * Draws an argument that takes an expression: a field, an input, or a value typed in. It opens the
 * list it is picked from, which offers what fits the method's parameter, as the engine says it
 * does. Every method's argument of this kind is drawn here.
 */
function ArgumentSlot({
  call,
  rule,
  callPath,
  place,
  frame,
  wrong,
}: ArgumentSlotProps): React.JSX.Element {
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const outer = usePositions();
  const list = useListState();
  const edits = useStepEdits();
  const step = useStepAt();
  const { parameter } = rule;
  const parameterPath = under(callPath, parameterSegment(parameter.name));
  const given = givenOf(call, parameter.name);
  const many = parameter.shape.shape === 'many';
  // The expression the argument is: a list of them holds the one the editor draws first.
  const held = many ? (given?.kind === 'many' ? given.items[0] : undefined) : given;
  const expression = expressionOf(held);
  const argumentPath = many ? under(parameterPath, itemSegment(0)) : parameterPath;
  const shown = shownArgument(expression, given !== undefined, parameter.shown);
  const column =
    expression === null
      ? null
      : (() => {
          const shape = shapeOf(expression);
          return shape.kind === 'column' ? findColumn(frame, shape.name) : null;
        })();
  // The argument stands in the context of the chain, and is the argument of the method where
  // the engine has reported where the method stands.
  const start = within(outer, parameter.position);
  const slot: FitSlot | null =
    start.length === 0 && place === null
      ? null
      : {
          start: start.length === 0 ? null : start,
          argument:
            place === null
              ? null
              : { entry: call.entry, parameter: parameter.name, place: placeRequest(place) },
        };
  const fits = useCandidateFits(slot, list.open, frame, node);
  const readings = useReadTyped(
    list.open ? { text: list.typed, typed: { kind: 'expression' }, slot } : null,
  );
  /** Puts what was picked where the argument is, or makes the argument if it was not given. */
  const set = (request: ValueRequest): void => {
    if (given === undefined) {
      void edits.setArgument(parameterPath, many ? listArgument([request]) : request);
    } else if (many && given.kind === 'many' && given.items.length === 0) {
      void edits.insertItem(parameterPath, 0, request);
    } else {
      void edits.setArgument(argumentPath, request);
    }
  };
  const offers = (typed: string): OfferGroup[] => {
    const rows = {
      frame,
      node,
      typed,
      fits,
      readings,
      inputs: OFFERED_INPUTS,
      onLiteral: (picked: LiteralView) => {
        set(literalExpression(picked));
      },
      onInput: (name: string) => {
        set(inputArgument(name));
      },
      onField: (name: string) => {
        set(fieldArgument(name));
      },
    };
    return listedGroups(
      { word: 'argument' },
      {
        values: () => [valueGroup(rows)],
        inputs: () => [inputGroup(rows)],
        fields: () => [fieldGroup(rows)],
      },
    );
  };
  return (
    <Hint lines={fieldHint(column)} code={false}>
      <Slot
        label={shown.label}
        tone={shown.tone}
        look={shown.look}
        wrong={wrong || shown.wrong}
        openNow={fresh.id === freshKey(step.id, callPath)}
        describedAs={`${rule.shown}: ${parameter.label}`}
        placeholder="Type a value, or search the fields"
        offers={offers}
        onOpenChange={list.onOpenChange}
        onType={list.onType}
      />
    </Hint>
  );
}

interface MethodLinkProps {
  /** The way to the call of the method in the step. */
  readonly path: Path;
  /** Where the method is among the links of its expression. */
  readonly index: number;
  readonly call: CallView;
  /** The dtypes after the chain's start and each of its methods, when the engine has reported them. */
  readonly dtypes: ChainDtypes | undefined;
  readonly frame: Frame;
  /** Opens the list of methods to put one after this one. */
  readonly onAddAfter: (piece: HTMLElement) => void;
  /** The group the method may be dragged within. */
  readonly group: string;
}

/** Draws one method of a chain as a piece behind its caret. */
function MethodLink({
  path,
  index,
  call,
  dtypes,
  frame,
  onAddAfter,
  group,
}: MethodLinkProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const wrongAt = useIsWrongAt();
  const toggle = useOptionToggle(path);
  const entry = catalogue.entries.get(call.entry) ?? null;
  const rule = argumentRule(entry);
  // What the catalogue calls the method; a method it lacks keeps its Polars name.
  const shown = entry?.name ?? methodNameOf(call.entry) ?? call.entry;
  const dtypeHint = linkHint(dtypes, index);
  // Where the method stands, as the engine reported it: what may follow what comes before it.
  const place = dtypes?.places[index] ?? null;
  const wrong = wrongAt(path);
  const list = useListState();
  const offered = useOffers(list.open && place !== null ? placeRequest(place) : null);
  const optionsAsked = useOptions(
    list.open && entry !== null
      ? {
          entry: entry.id,
          chosen: chosenOptions(entry, call.arguments),
        }
      : null,
  );
  const { ref, handleRef, isDragSource } = useSortable({
    id: pathKey(path),
    index,
    group,
    type: `part:${group}`,
    accept: `part:${group}`,
    sensors: PART_SENSORS,
  });
  const swap = (picked: PreparedOffer): void => {
    void edits.swapLink(path, picked.id, standingOf(place));
  };
  const parameters = methodOptionParameters(entry);
  const given = new Set(call.arguments.map((held) => held.parameter));
  const actions = actionsOf('method');
  return (
    <span ref={ref} className="dg-link">
      <Seam />
      <Piece
        dragged={isDragSource}
        remove={
          actions.remove
            ? {
                label: `Remove ${shown}`,
                onPress: () => {
                  void edits.removeLink(path);
                },
              }
            : null
        }
        add={actions.add ? { label: `Add a method after ${shown}`, onPress: onAddAfter } : null}
      >
        <Hint
          lines={[
            `${shown} · ${entry?.hint ?? 'not in the catalogue'}`,
            ...(dtypeHint === null ? [] : [dtypeHint]),
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
            offers={(typed) =>
              // The parameters the method can be given, then the methods it can be swapped for.
              listedGroups(
                { word: 'method', options: parameters.length > 0 },
                {
                  options: () => [optionOffers(parameters, given, typed, toggle, optionsAsked)],
                  methods: () =>
                    methodGroups(catalogue, offered, place !== null, typed, (_name, picked) => {
                      swap(picked);
                    }),
                },
              )
            }
            onOpenChange={list.onOpenChange}
            onType={list.onType}
            buttonRef={handleRef}
          />
        </Hint>
        {rule !== null && (
          <ArgumentSlot
            call={call}
            rule={rule}
            callPath={path}
            place={place}
            frame={frame}
            wrong={wrong}
          />
        )}
        <OptionRows
          entry={entry}
          call={call}
          callPath={path}
          parameters={parameters}
          around={{
            frame,
            items: () => [],
            method: entry === null ? undefined : { entry: entry.id, place },
          }}
          layout="inline"
        />
        {entry === null && (
          <span className="font-sans text-xs text-wrong">{`not in the catalogue as ${methodId(shown)}`}</span>
        )}
      </Piece>
    </span>
  );
}

/** Where a method is about to be added: its place among the links and the piece its list opens from. */
interface Insertion {
  readonly at: number;
  readonly anchor: HTMLElement;
}

/** Where the list that adds a method is, and which place of an expression the method is added at. */
export interface MethodInsertionProps {
  /** The way to the expression in the step. */
  readonly path: Path;
  /** The place among the links of the expression the method is added at. */
  readonly at: number;
  /** The piece the list opens from. */
  readonly anchor: HTMLElement;
  /** Says the list was closed, with or without a pick. */
  readonly onDone: () => void;
}

/**
 * Draws the list of methods that may follow what stands before the place `at` of the expression
 * at `path`, as the engine says of the place its walk reported, and adds the one that is picked.
 */
export function MethodInsertion({
  path,
  at,
  anchor,
  onDone,
}: MethodInsertionProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const fresh = useContext(FreshContext);
  const edits = useStepEdits();
  const step = useStepAt();
  const dtypes = usePreparedStep()?.chains.get(pathKey(path));
  // What may stand where a method would be added, as the engine says of the place it reported.
  const place = dtypes?.places[at] ?? null;
  const inserting = useOffers(place === null ? null : placeRequest(place));
  return (
    <Popover anchor={anchor} onDismiss={onDone}>
      <OfferList
        placeholder="Search the methods"
        groups={(typed) =>
          listedGroups(
            { word: 'insertion' },
            {
              methods: () =>
                methodGroups(catalogue, inserting, place !== null, typed, (_name, picked) => {
                  fresh.mark(freshKey(step.id, under(path, linkSegment(at))));
                  void edits.insertLink(path, at, picked.id, standingOf(place));
                }),
            },
          )
        }
        keepOpen={false}
        wide={false}
        onDone={onDone}
      />
    </Popover>
  );
}

/** What a chain shows and reports. */
export interface ChainBlocksProps {
  /** The expression the document holds. */
  readonly expression: ExpressionView;
  /** The way to the expression in the step. */
  readonly path: Path;
  /** The columns the chain's fields are picked from. */
  readonly frame: Frame;
  /** Takes the whole chain away, leaving its place for something else. */
  readonly onRemove: () => void;
}

/** Returns the word a condition's entry is shown by, with its hint, and whether the catalogue lacks it. */
function wordOfCondition(
  catalogue: Catalogue,
  which: ConditionWord,
): { text: string; hint: string; lacking: boolean } {
  const found = conditionEntry(catalogue, which);
  return {
    text: found?.name ?? which,
    hint: found?.hint ?? 'not in the catalogue',
    lacking: found === null,
  };
}

/**
 * Draws `expression` as what it starts from and its methods. The parts sit directly in their row,
 * so a long chain breaks before a caret and never drops to a new line as a whole.
 */
export function ChainBlocks({
  expression,
  path,
  frame,
  onRemove,
}: ChainBlocksProps): React.JSX.Element {
  const settings = useContext(DragSettingsContext);
  const catalogue = useCatalogue();
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const prepared = usePreparedStep();
  const edits = useStepEdits();
  const step = useStepAt();
  const wrongAt = useIsWrongAt();
  const positions = usePositions();
  const dtypes = prepared?.chains.get(pathKey(path));
  const [insertion, setInsertion] = useState<Insertion | null>(null);
  // A chain made by carrying a field on opens its list of methods as soon as it is shown.
  const [opensMethods, setOpensMethods] = useState(
    () => fresh.id === `${freshKey(step.id, path)}:method`,
  );
  const closeList = useCallback(() => {
    setInsertion(null);
    setOpensMethods(false);
  }, []);
  const { start, links } = expression;
  const chainBody = chainBodyOf(expression);
  const when = chainBody.body === 'when' ? chainBody.layout : null;
  // The links of a condition come first, and the methods after them.
  const before = when?.before ?? 0;
  const methods = links.slice(before);
  const startPath = under(path, START_SEGMENT);
  const startsWrong = wrongAt(startPath);
  const startCall = start.kind === 'call' ? start.call : null;
  const name = startCall === null ? null : fieldNameOf(startCall);
  const startName = name ?? (start.kind === 'input' ? start.name : '');
  const unset = start.kind === 'hole';
  const startInput = start.kind === 'input' ? findInput(node, start.name) : null;
  const column = name === null ? null : findColumn(frame, name);
  // The word the chain starts from, which that first list of methods opens against.
  const [startWord, setStartWord] = useState<HTMLElement | null>(null);
  const adding: Insertion | null =
    insertion ??
    (opensMethods && startWord !== null ? { at: links.length, anchor: startWord } : null);
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
  const addAt = (at: number): ((anchor: HTMLElement) => void) => {
    return (anchor) => {
      setOpensMethods(false);
      setInsertion({ at, anchor });
    };
  };
  // A chain is taken away by the word it starts from, and a method is added after it.
  const chainActions = actionsOf('chainStart');
  const removeChain = chainActions.remove
    ? { label: 'Remove this chain', onPress: onRemove }
    : null;
  const addAfter = (
    label: string,
    at: number,
  ): { label: string; onPress: (anchor: HTMLElement) => void } | null =>
    chainActions.add ? { label, onPress: addAt(at) } : null;
  const startOffer = (typed: string): OfferGroup[] => {
    const rows = {
      frame,
      node,
      typed,
      fits: startFits,
      inputs: OFFERED_INPUTS,
      onInput: (picked: string) => {
        void edits.setStart(path, { kind: 'input', name: picked });
      },
      onField: (picked: string) => {
        void edits.setStart(path, { kind: 'field', name: picked });
      },
    };
    return listedGroups(
      { word: 'chainStart' },
      {
        functions: () =>
          startFunctions(catalogue, startOffers, startKnown, typed, (id) => {
            if (id === 'polars.when') {
              fresh.mark(
                freshKey(
                  step.id,
                  under(startPath, parameterSegment(CONDITION_TESTS), itemSegment(0)),
                ),
              );
            } else {
              fresh.mark(`${freshKey(step.id, path)}:method`);
            }
            void edits.startWith(path, id);
          }),
        inputs: () => [inputGroup(rows)],
        fields: () => [fieldGroup(rows)],
      },
    );
  };

  // Each chain sorts its own methods, so a method cannot leave its chain.
  const group = `chain:${step.id}:${pathKey(path)}`;
  const methodsDrawn = (
    <DragDropProvider
      {...settings}
      onDragEnd={(event) => {
        const dragged = event.operation.source;
        if (event.canceled || !isSortable(dragged)) return;
        if (dragged.index !== dragged.initialIndex) {
          void edits.moveLink(under(path, linkSegment(dragged.initialIndex)), dragged.index);
        }
      }}
    >
      {methods.map((call, at) => {
        const index = before + at;
        return (
          <MethodLink
            key={index}
            path={under(path, linkSegment(index))}
            index={index}
            call={call}
            dtypes={dtypes}
            frame={frame}
            group={group}
            onAddAfter={addAt(index + 1)}
          />
        );
      })}
    </DragDropProvider>
  );
  // What a condition holds stands in a position inside the expression: its predicates in one, its
  // values in another.
  const given = (link: number, label: string, which: 'then' | 'otherwise'): React.JSX.Element => {
    const call = links[link];
    const valuePath = under(path, linkSegment(link), parameterSegment('statement'));
    return (
      <ValueSlot
        expression={expressionOf(call === undefined ? undefined : givenOf(call, 'statement'))}
        path={valuePath}
        frame={frame}
        label={label}
        positions={within(
          positions,
          positionOfParameter(conditionEntry(catalogue, which), 'statement'),
        )}
        argument={null}
        onSet={(request) => {
          void edits.setArgument(valuePath, request);
        }}
      />
    );
  };
  // The word `element` comes from the catalogue; one it lacks keeps its Polars name and says so.
  const elementStart = elementEntry(catalogue);
  const elementWord = elementStart?.name ?? 'element';
  const elementHint = elementStart?.hint ?? 'not in the catalogue';
  const [whenWord, thenWord, otherwiseWord] = [
    wordOfCondition(catalogue, 'when'),
    wordOfCondition(catalogue, 'then'),
    wordOfCondition(catalogue, 'otherwise'),
  ];
  const lacking = (['when', 'then', 'otherwise'] as const)
    .filter((which) => conditionEntry(catalogue, which) === null)
    .map((which) => conditionId(which));
  const startSlot = (
    label: string,
    tone: Tone,
    look: Look,
    lines: string[],
    describedAs: string,
  ): React.JSX.Element => (
    <Piece
      remove={removeChain}
      add={addAfter(unset ? 'Add a method after the field' : `Add a method after ${label}`, before)}
    >
      <Hint lines={lines} code={false}>
        <Slot
          label={label}
          tone={tone}
          look={look}
          wrong={startsWrong}
          names
          openNow={fresh.id === freshKey(step.id, path)}
          buttonRef={setStartWord}
          describedAs={describedAs}
          placeholder="Search the fields and functions"
          offers={startOffer}
          onOpenChange={startList.onOpenChange}
          onType={startList.onType}
        />
      </Hint>
    </Piece>
  );

  let body: React.JSX.Element;
  if (when !== null) {
    // A when is set out on lines: its condition, what it gives, and the chain carrying on.
    body = (
      <Lines>
        <Line>
          <Piece remove={removeChain} add={addAfter(`Add a method after ${whenWord.text}`, before)}>
            <Hint lines={[`${whenWord.text} · ${whenWord.hint}`]} code={false}>
              <Slot
                label={whenWord.text}
                tone="function"
                look="plain"
                names
                buttonRef={setStartWord}
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
            path={under(startPath, parameterSegment(CONDITION_TESTS))}
            argument={when.predicates}
            frame={frame}
            positions={within(
              positions,
              positionOfParameter(conditionEntry(catalogue, 'when'), CONDITION_TESTS),
            )}
            tone="function"
          />
        </Line>
        <Line>
          <Word title={thenWord.hint}>{thenWord.text}</Word>
          {given(0, 'The value where the condition holds', 'then')}
        </Line>
        <Line>
          <Word title={otherwiseWord.hint}>{otherwiseWord.text}</Word>
          {given(1, 'The value where the condition does not hold', 'otherwise')}
        </Line>
        {lacking.length > 0 && (
          <Line>
            <span className="font-sans text-xs text-wrong">{`not in the catalogue as ${lacking.join(', ')}`}</span>
          </Line>
        )}
        {methods.length > 0 && <Line>{methodsDrawn}</Line>}
      </Lines>
    );
  } else if (chainBody.body === 'element') {
    body = (
      <>
        <Piece remove={removeChain} add={addAfter(`Add a method after ${elementWord}`, 0)}>
          <Hint lines={[`${elementWord} · ${elementHint}`]} code={false}>
            <Slot
              label={elementWord}
              tone="function"
              look="plain"
              wrong={startsWrong || elementStart === null}
              names
              buttonRef={setStartWord}
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
        {methodsDrawn}
      </>
    );
  } else if (chainBody.body === 'function' && startCall !== null) {
    // A function the editor has no word of its own for is drawn as its name, with what it was given.
    const entry = catalogue.entries.get(startCall.entry) ?? null;
    body = (
      <>
        {startSlot(
          entry?.name ?? startCall.entry,
          'function',
          'plain',
          [`${entry?.name ?? startCall.entry} · ${entry?.hint ?? 'not in the catalogue'}`],
          'The function the chain starts from',
        )}
        <OptionRows
          entry={entry}
          call={startCall}
          callPath={startPath}
          parameters={entry?.parameters ?? []}
          around={{ frame, items: () => [] }}
          layout="inline"
        />
        {methodsDrawn}
      </>
    );
  } else if (chainBody.body === 'typed') {
    // A number or text typed in is drawn as the word it is shown by, with its methods after it.
    body = (
      <>
        {startSlot(
          chainBody.shown,
          'value',
          chainBody.text ? 'quoted' : 'plain',
          [],
          'What the chain starts from',
        )}
        {methodsDrawn}
      </>
    );
  } else {
    body = (
      <>
        {startSlot(
          unset ? 'field' : startName,
          unset ? 'value' : start.kind === 'input' ? 'input' : 'column',
          unset ? 'none' : 'plain',
          startInput === null ? fieldHint(column) : inputLines(startInput),
          'What the chain starts from',
        )}
        {methodsDrawn}
      </>
    );
  }

  return (
    <>
      {body}
      {adding !== null && (
        <MethodInsertion path={path} at={adding.at} anchor={adding.anchor} onDone={closeList} />
      )}
    </>
  );
}

/** Returns the argument of a call as an expression, which the `when` layout draws, or `null`. */
export function argumentExpression(call: CallView, parameter: string): ExpressionView | null {
  const held: ArgumentView | undefined = givenOf(call, parameter);
  return expressionOf(held);
}
