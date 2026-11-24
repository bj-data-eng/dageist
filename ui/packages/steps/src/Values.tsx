/**
 * @file Values and the conditions made of them. A value is a field, a connected input, something
 * typed, or a chain of its own, written where it stands. A comparison is a field, a comparison and
 * a value as three words. A condition is a list of them on lines beneath its word, all of which
 * must hold; one comparison is a list of one. What is drawn is what the document holds, and each
 * pick is sent as an edit.
 */

import { useCallback, useContext, useState } from 'react';

import type { Look, OfferGroup, Tone } from '@dageist/design';
import { Hint, OfferList, Piece, Popover, Slot } from '@dageist/design';
import type {
  ArgumentRequest,
  ArgumentView,
  LiteralView,
  PositionView,
  ValueRequest,
} from '@dageist/host';

import type { FitSlot } from './ask';
import { useCandidateFits, useOffers, useReadTyped } from './ask';
import { ChainBlocks } from './Chain';
import { useStepAt, useStepEdits } from './editor';
import { useCatalogue, useNode } from './EngineProvider';
import { usePreparedStep } from './engine';
import { fieldGroup, inputGroup, refusalOf, startFunctions, valueGroup } from './engineLists';
import { FreshContext, Line, Lines } from './Lists';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import { PositionsContext, useListState, usePositions } from './positions';
import type { ExpressionView, Path, TestView } from './model/doc';
import {
  expressionOf,
  isWithin,
  itemSegment,
  linkSegment,
  parameterSegment,
  pathKey,
  shapeOf,
  testOf,
  under,
} from './model/doc';
import type { Frame } from './model/frame';
import { findColumn } from './model/frame';
import { findInput, inputLines } from './model/inputs';
import { comparisonOption, comparisonOptions } from './model/catalogue';
import { NO_POSITIONS, placeRequest, startRequest } from './model/place';
import {
  HOLE,
  fieldArgument,
  inputArgument,
  listArgument,
  literalExpression,
} from './model/requests';

/** Returns whether the engine's report puts what is wrong with the step in or at `path`. */
export function useIsWrongAt(): (path: Path) => boolean {
  const at = usePreparedStep()?.problemAt ?? null;
  return (path) => at !== null && isWithin(at, path);
}

/** What a value slot shows and reports. */
export interface ValueSlotProps {
  /** What the document holds, or `null` where it holds nothing there yet. */
  readonly expression: ExpressionView | null;
  /** The way to the value in the step, or `null` for one that is not in the document yet. */
  readonly path: Path | null;
  readonly frame: Frame;
  /** What the value is for, for assistive technology. */
  readonly label: string;
  /**
   * The positions that hold an expression picked here, which is where the engine's tables are
   * asked what may start it; `null` where no named position does.
   */
  readonly positions: readonly PositionView[] | null;
  /** The argument of a method that is picked here, which the engine says what fits; `null` where it is not one. */
  readonly argument: ArgumentRequest | null;
  /** The id that, when it is what was just added, opens the slot's list as soon as it is shown. */
  readonly openId?: string | undefined;
  /** Takes the value away from where it stands, when it is one of several; otherwise it can only be unset. */
  readonly onRemove?: (() => void) | undefined;
  /** Says what the person picked for it. */
  readonly onSet: (request: ValueRequest) => void;
}

/** Returns how a typed value is shown as a block: in the words the engine gave it, text between quotation marks the block draws. */
function literalBlock(shown: string, text: boolean): { label: string; look: Look } {
  return { label: shown, look: text ? 'quoted' : 'plain' };
}

/**
 * Draws a value as a word in the colour of what it is, which opens the list it is picked from: a
 * chain of its own, something typed, a connected input, or a field. The list never offers the
 * values a field holds: a column can hold millions of them. A chain is written where the value
 * stands.
 */
export function ValueSlot({
  expression,
  path,
  frame,
  label,
  positions,
  argument,
  openId,
  onRemove,
  onSet,
}: ValueSlotProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const edits = useStepEdits();
  const list = useListState();
  const shape = expression === null ? ({ kind: 'unset' } as const) : shapeOf(expression);
  // What may stand here, as the engine says of the place: the functions that may start an
  // expression, and which of the fields, inputs and values typed in fit.
  const startKnown = positions !== null && positions.length > 0;
  const startOffers = useOffers(list.open && startKnown ? startRequest(positions) : null);
  const slot: FitSlot | null =
    !startKnown && argument === null ? null : { start: startKnown ? positions : null, argument };
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
        onSet(literalExpression(picked));
      },
      onInput: (name: string) => {
        onSet(inputArgument(name));
      },
      onField: (name: string) => {
        onSet(fieldArgument(name));
      },
    };
    // A chain of its own starts from a field, which the engine says may stand here or not.
    const chainRefusal = refusalOf(startOffers, 'polars.col');
    const chain = offer(
      '…',
      'function',
      'a chain of its own',
      () => {
        onSet(HOLE);
        if (openId !== undefined) fresh.mark(openId);
      },
      { id: 'chain', look: 'dots', act: true, reason: chainRefusal },
    );
    // The groups go from the shortest to the longest, so a long one never hides a short one.
    return [
      titled(null, typed === '' ? [chain] : []),
      valueGroup({ ...rows, readings }),
      inputGroup(rows),
      fieldGroup(rows),
      // A function starts the expression that is in the document, so it has a place to start at.
      ...(path === null
        ? []
        : startFunctions(catalogue, startOffers, startKnown, typed, (id) => {
            void edits.startWith(path, id);
          })),
    ];
  };

  if (expression !== null && path !== null && (shape.kind === 'column' || shape.kind === 'chain')) {
    // A chain is written where it stands. Its start takes the whole chain away.
    return (
      <PositionsContext.Provider value={positions ?? NO_POSITIONS}>
        <ChainBlocks
          expression={expression}
          path={path}
          frame={frame}
          onRemove={
            onRemove ??
            (() => {
              onSet(HOLE);
            })
          }
        />
      </PositionsContext.Provider>
    );
  }

  let shown: { label: string; tone: Tone; look: Look; wrong: boolean; lines: string[] };
  if (shape.kind === 'input') {
    const input = findInput(node, shape.name);
    shown = {
      label: shape.name,
      tone: 'input',
      look: 'plain',
      wrong: input === null,
      lines: input === null ? [] : inputLines(input),
    };
  } else if (shape.kind === 'column') {
    const column = findColumn(frame, shape.name);
    shown = {
      label: shape.name,
      tone: 'column',
      look: 'plain',
      wrong: column === null,
      lines: fieldHint(column),
    };
  } else if (shape.kind === 'literal') {
    shown = { ...literalBlock(shape.shown, shape.text), tone: 'value', wrong: false, lines: [] };
  } else {
    // Nothing is picked yet, so the slot says what goes there, in the colour of what is missing.
    shown = { label: 'value', tone: 'value', look: 'none', wrong: true, lines: [] };
  }
  return (
    <Piece
      remove={
        onRemove === undefined
          ? null
          : {
              label: 'Remove this one',
              onPress: onRemove,
            }
      }
      add={null}
    >
      <Hint lines={shown.lines} code={false}>
        <Slot
          label={shown.label}
          tone={shown.tone}
          look={shown.look}
          wrong={shown.wrong}
          openNow={openId !== undefined && fresh.id === openId}
          names
          describedAs={label}
          placeholder="Type a value, or search"
          offers={offers}
          onOpenChange={list.onOpenChange}
          onType={list.onType}
        />
      </Hint>
    </Piece>
  );
}

/** What a comparison shows and reports. */
interface TestBlocksProps {
  readonly test: TestView;
  /** The way to the expression that is the test. */
  readonly path: Path;
  readonly frame: Frame;
}

/** Draws a comparison as three blocks: a field, a comparison and a value. */
function TestBlocks({ test, path, frame }: TestBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const catalogue = useCatalogue();
  const prepared = usePreparedStep();
  const edits = useStepEdits();
  const step = useStepAt();
  const fieldHint = useFieldHint();
  const positions = usePositions();
  const wrongAt = useIsWrongAt();
  const column = test.column === null ? null : findColumn(frame, test.column);
  const linkPath = under(path, linkSegment(0));
  const valuePath = under(linkPath, parameterSegment('other'));
  // The value is the argument of the method the comparison is written as, which follows the
  // field: the engine says what fits it.
  const argument: ArgumentRequest | null =
    column === null || positions.length === 0
      ? null
      : {
          entry: test.comparison.entry,
          parameter: 'other',
          place: { positions, after: { kind: 'value', dtype: column.dtype, shape: 'many' } },
        };
  const standing = prepared?.chains.get(pathKey(path))?.places[0] ?? null;

  // What each comparison is written as and means is the engine's, as the catalogue lists it.
  const held = comparisonOption(catalogue, test.comparison.entry);
  return (
    <>
      <Hint lines={fieldHint(column)} code={false}>
        <Slot
          label={test.column ?? 'field'}
          tone={test.column === null ? 'value' : 'column'}
          look={test.column === null ? 'none' : 'plain'}
          wrong={test.column === null || column === null || wrongAt(under(path, { kind: 'start' }))}
          openNow={fresh.id === `${step.id}:${pathKey(path)}`}
          describedAs="The field that is compared"
          placeholder="Search the fields"
          offers={(typed) => [
            counted(
              'Fields',
              fieldOffers(
                frame,
                typed,
                anyField((name) => {
                  void edits.setStart(path, { kind: 'field', name });
                }),
              ),
            ),
          ]}
        />
      </Hint>
      <Slot
        label={held.sign}
        tone="compare"
        look="compare"
        wrong={wrongAt(linkPath)}
        names
        describedAs={`Comparison: ${held.hint}`}
        placeholder="Search the comparisons"
        offers={(typed) => [
          titled(
            'Comparison',
            comparisonOptions(catalogue)
              .filter((option) => matches(typed, option.entry, option.sign, option.hint))
              .map((option) =>
                offer(
                  option.sign,
                  'compare',
                  option.hint,
                  () => {
                    void edits.swapLink(
                      linkPath,
                      option.entry,
                      standing === null ? null : placeRequest(standing),
                    );
                  },
                  { id: option.entry, look: 'compare' },
                ),
              ),
          ),
        ]}
      />
      <ValueSlot
        expression={expressionOf(test.other)}
        path={valuePath}
        frame={frame}
        label={
          test.column === null
            ? 'The value it is compared with'
            : `Value compared with ${test.column}`
        }
        positions={positions.length === 0 ? null : positions}
        argument={argument}
        onSet={(request) => {
          void edits.setArgument(valuePath, request);
        }}
      />
    </>
  );
}

/** What the lines of a condition show and report. */
interface ConditionProps {
  /** The way to the list of conditions in the step. */
  readonly path: Path;
  /** What the document holds for the list, or `undefined` where it is not given. */
  readonly argument: ArgumentView | undefined;
  readonly frame: Frame;
  /** The positions that hold the condition: a filter's predicates, or a `when`'s. */
  readonly positions: readonly PositionView[];
  /** The colour of the word that heads the list: that of the step, or a function's under a `when`. */
  readonly tone: Tone;
}

/** What goes where a line is added: a comparison, or a chain. */
function lineOffers(typed: string, onTest: () => void, onChain: () => void): OfferGroup[] {
  return [
    titled(
      'Add a line',
      [
        offer('field == value', 'value', 'a comparison', onTest, { look: 'none', act: true }),
        offer('…', 'function', 'a chain of its own, ending in a comparison', onChain, {
          id: 'chain',
          look: 'dots',
          act: true,
        }),
      ].filter((row) => matches(typed, row.label, row.note)),
    ),
  ];
}

/**
 * Draws a condition where it stands: the word `all of`, which heads the lines set in beneath it, each a
 * comparison or a chain of its own. The word's list ends with adding a line and with taking the
 * whole condition away, and removing its last line leaves a condition with nothing in it, which is
 * an empty slot that offers how to start one again.
 */
export function ConditionBlocks({
  path,
  argument,
  frame,
  positions,
  tone,
}: ConditionProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const edits = useStepEdits();
  const fresh = useContext(FreshContext);
  const step = useStepAt();
  const [adding, setAdding] = useState<{ at: number; anchor: HTMLElement } | null>(null);
  const closeList = useCallback(() => {
    setAdding(null);
  }, []);
  const comparisons = catalogue.comparisons;
  const items = argument?.kind === 'many' ? argument.items : [];

  /** Puts a new test at `at`, in the list as it is: given, or about to be. */
  const addTest = (at: number): void => {
    fresh.mark(`${step.id}:${pathKey(under(path, itemSegment(at)))}`);
    if (argument === undefined) {
      // The list is not in the document yet, so it is made first, and the test goes into it.
      void edits.setArgument(path, listArgument([])).then(() => edits.insertTest(path, at));
    } else {
      void edits.insertTest(path, at);
    }
  };
  const addChain = (at: number): void => {
    fresh.mark(`${step.id}:${pathKey(under(path, itemSegment(at)))}`);
    if (argument === undefined) void edits.setArgument(path, listArgument([HOLE]));
    else void edits.insertItem(path, at, HOLE);
  };

  const body =
    items.length === 0 ? (
      <Slot
        label="condition"
        tone="value"
        look="none"
        wrong
        describedAs="The condition: nothing is picked yet"
        placeholder="Search"
        offers={(typed) => [
          titled(
            'A list of comparisons',
            [
              offer(
                'all of',
                'function',
                'every line must hold',
                () => {
                  addTest(0);
                },
                { act: true },
              ),
            ].filter((row) => matches(typed, row.label, row.note)),
          ),
          titled(
            'A chain',
            [
              offer(
                '…',
                'function',
                'a chain of its own, ending in a comparison',
                () => {
                  addChain(0);
                },
                { id: 'chain', look: 'dots', act: true },
              ),
            ].filter((row) => matches(typed, row.label, row.note)),
          ),
        ]}
      />
    ) : (
      <>
        <Piece
          remove={{
            label: 'Remove the condition',
            onPress: () => {
              void edits.setArgument(path, listArgument([]));
            },
          }}
          add={{
            label: 'Add a line to all of',
            onPress: (anchor: HTMLElement) => {
              setAdding({ at: 0, anchor });
            },
          }}
        >
          <span data-tone={tone} className="dg-block dg-fixed">
            all of
          </span>
        </Piece>
        <Lines>
          {items.map((item, at) => {
            const itemPath = under(path, itemSegment(at));
            const expression = expressionOf(item);
            const test = expression === null ? null : testOf(expression, comparisons);
            const remove = {
              label: 'Remove this line',
              onPress: () => {
                void edits.removeItem(itemPath);
              },
            };
            const add = {
              label: 'Add a line after this one, joined by and',
              onPress: (anchor: HTMLElement) => {
                setAdding({ at: at + 1, anchor });
              },
            };
            if (expression === null) return null;
            if (test === null) {
              // A chain is a line of its own, and its start takes the line away.
              return (
                <Line key={at}>
                  <ChainBlocks
                    expression={expression}
                    path={itemPath}
                    frame={frame}
                    onRemove={remove.onPress}
                  />
                </Line>
              );
            }
            return (
              <Line key={at}>
                <Piece remove={remove} add={add}>
                  <TestBlocks test={test} path={itemPath} frame={frame} />
                </Piece>
              </Line>
            );
          })}
        </Lines>
        {adding !== null && (
          <Popover anchor={adding.anchor} onDismiss={closeList}>
            <OfferList
              placeholder="Search"
              groups={(typed) =>
                lineOffers(
                  typed,
                  () => {
                    addTest(adding.at);
                  },
                  () => {
                    addChain(adding.at);
                  },
                )
              }
              keepOpen={false}
              wide={false}
              onDone={closeList}
            />
          </Popover>
        )}
      </>
    );
  return <PositionsContext.Provider value={positions}>{body}</PositionsContext.Provider>;
}
