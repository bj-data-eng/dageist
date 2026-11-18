/**
 * @file Values and the conditions made of them. A value is a field, a
 * connected input, something typed, or a chain of its own, written where it
 * stands. A comparison is a field, a comparison and a value as three words.
 * A condition is always a list of them on lines beneath its word, where all
 * must hold or any may, with a group as lines set in again. One comparison
 * is a list of one.
 */

import { useCallback, useContext, useState } from 'react';

import type { Look, Offer, OfferGroup, Tone } from '@dageist/design';
import { Hint, OfferList, Piece, Popover, Slot } from '@dageist/design';
import type { ArgumentRequest, LiteralView, PositionView } from '@dageist/host';

import type { FitSlot } from './ask';
import { useCandidateFits, useOffers, useReadTyped } from './ask';
import { useCatalogue, useNode } from './EngineProvider';
import { usePreparedStep } from './engine';
import { comparisonEntryId, wrongId } from './model/document';
import { ChainBlocks } from './Chain';
import { fieldGroup, inputGroup, refusalOf, startFunctions, valueGroup } from './engineLists';
import { FreshContext, Line, Lines } from './Lists';
import { PositionsContext, useListState, usePositions } from './positions';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { Frame } from './model/frame';
import { findColumn } from './model/frame';
import { NO_POSITIONS, startRequest } from './model/place';
import { nextId } from './model/ids';
import { findInput, inputLines } from './model/inputs';
import { literalValue } from './model/literal';
import type {
  ChainCondition,
  Comparison,
  Condition,
  Group,
  LiteralValue,
  Test,
  Value,
} from './model/source';
import { IS_IN } from './model/source';
import type { Catalogue } from './model/catalogue';
import { comparisonChoices, comparisonOption } from './model/catalogue';
import { emptyTest } from './model/conditions';

/** A value that is still to be picked. */
const UNSET_VALUE: Value = { kind: 'unset' };

/** Returns how a typed value is shown as a block: in the words the engine gave it, text between quotation marks the block draws. */
function literalBlock(value: LiteralValue): { label: string; look: Look } {
  if (value.value === null) return { label: 'None', look: 'none' };
  return { label: value.shown, look: typeof value.value === 'number' ? 'plain' : 'quoted' };
}

/** Returns a condition written as a chain, with nothing picked yet. */
function emptyChainCondition(): ChainCondition {
  return {
    kind: 'chain',
    id: nextId('chain'),
    chain: { source: { kind: 'column', column: '' }, methods: [] },
  };
}

/** Returns the row that offers a condition written as a chain of its own. */
function chainOffer(onAdd: (condition: ChainCondition) => void): Offer {
  return offer(
    '…',
    'function',
    'a chain of its own, ending in a comparison',
    () => {
      onAdd(emptyChainCondition());
    },
    { id: 'chain', look: 'dots', act: true },
  );
}

/** What a value slot shows and reports. */
export interface ValueSlotProps {
  readonly value: Value;
  readonly frame: Frame;
  /** What the value is for, for assistive technology. */
  readonly label: string;
  /** Whether the slot takes a connected list, as `is_in` does, instead of a single value. */
  readonly lists: boolean;
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
  readonly onChange: (value: Value) => void;
}

/**
 * Draws a value as a word in the colour of what it is, which opens the list
 * it is picked from: a chain of its own, something typed, a connected input,
 * or a field. The list never offers the values a field holds: a column can
 * hold millions of them. A chain is written where the value stands.
 */
export function ValueSlot({
  value,
  frame,
  label,
  lists,
  positions,
  argument,
  openId,
  onRemove,
  onChange,
}: ValueSlotProps): React.JSX.Element {
  const catalogue = useCatalogue();
  const node = useNode();
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const list = useListState();
  // What may stand here, as the engine says of the place: the functions that may start an
  // expression, and which of the fields, inputs and values typed in fit.
  const startKnown = positions !== null && positions.length > 0;
  const startOffers = useOffers(list.open && startKnown && !lists ? startRequest(positions) : null);
  const slot: FitSlot | null =
    lists || (!startKnown && argument === null)
      ? null
      : { start: startKnown ? positions : null, argument };
  const fits = useCandidateFits(slot, list.open, frame, node);
  const readings = useReadTyped(
    list.open && !lists ? { text: list.typed, typed: { kind: 'expression' }, slot } : null,
  );
  const offers = (typed: string): OfferGroup[] => {
    const connected = node.inputs
      .flatMap((input) => (input.kind === (lists ? 'list' : 'value') ? [input] : []))
      .filter((input) => matches(typed, input.name))
      .map((input) =>
        offer(input.name, 'input', lists ? `list of ${input.dtype}` : input.dtype, () => {
          onChange({ kind: 'input', name: input.name });
        }),
      );
    // The values of `is_in` are lists, which the engine has no entry to judge yet, so they are offered
    // as they are connected.
    if (lists) return [counted('Inputs', connected)];

    const rows = {
      frame,
      node,
      typed,
      fits,
      readings,
      inputs: ['value', 'list'] as const,
      onLiteral: (picked: LiteralView) => {
        onChange(literalValue(picked));
      },
      onInput: (name: string) => {
        onChange({ kind: 'input', name });
      },
      onField: (column: string) => {
        onChange({ kind: 'column', column });
      },
    };
    // A chain of its own starts from a field, which the engine says may stand here or not.
    const chainRefusal = refusalOf(startOffers, 'polars.col');
    const chain = offer(
      '…',
      'function',
      'a chain of its own',
      () => {
        const id = nextId('chain');
        onChange({
          kind: 'chain',
          id,
          chain: { source: { kind: 'column', column: '' }, methods: [] },
        });
        fresh.mark(id);
      },
      { id: 'chain', look: 'dots', act: true, reason: chainRefusal },
    );
    const written = valueGroup(rows);
    const none = offer(
      'None',
      'value',
      'no value',
      () => {
        onChange({ kind: 'literal', value: null });
      },
      { id: 'none', look: 'none' },
    );
    // The groups go from the shortest to the longest, so a long one never hides a short one.
    return [
      titled(null, typed === '' ? [chain] : []),
      titled('Value', [...written.offers, none]),
      inputGroup(rows),
      fieldGroup(rows),
      ...startFunctions(catalogue, startOffers, startKnown, typed, (id) => {
        const made = nextId('chain');
        if (id === 'polars.when') {
          const arm = {
            id: nextId('arm'),
            condition: emptyTest(catalogue),
            returns: UNSET_VALUE,
          };
          onChange({
            kind: 'chain',
            id: made,
            chain: {
              source: { kind: 'when', arms: [arm], otherwise: UNSET_VALUE },
              methods: [],
            },
          });
          fresh.mark(arm.condition.id);
        } else {
          onChange({
            kind: 'chain',
            id: made,
            chain: { source: { kind: 'element' }, methods: [] },
          });
          // Its first method is the next thing to pick.
          fresh.mark(`${made}:method`);
        }
      }),
    ];
  };

  if (value.kind === 'chain') {
    // A chain is written where it stands. Its start takes the whole chain away.
    return (
      <PositionsContext.Provider value={positions ?? NO_POSITIONS}>
        <ChainBlocks
          chainId={value.id}
          chain={value.chain}
          frame={frame}
          onRemove={
            onRemove ??
            (() => {
              onChange({ kind: 'unset' });
            })
          }
          onChange={(chain) => {
            onChange({ kind: 'chain', id: value.id, chain });
          }}
        />
      </PositionsContext.Provider>
    );
  }

  let shown: { label: string; tone: Tone; look: Look; wrong: boolean; lines: string[] };
  if (value.kind === 'input') {
    const input = findInput(node, value.name);
    shown = {
      label: value.name === '' ? 'list' : value.name,
      tone: value.name === '' ? 'value' : 'input',
      look: value.name === '' ? 'none' : 'plain',
      wrong: value.name !== '' && input === null,
      lines: input === null ? [] : inputLines(input),
    };
  } else if (value.kind === 'column') {
    const column = findColumn(frame, value.column);
    shown = {
      label: value.column,
      tone: 'column',
      look: 'plain',
      wrong: column === null,
      lines: fieldHint(column),
    };
  } else if (value.kind === 'unset') {
    // Nothing is picked yet, so the slot says what goes there, in the colour of what is missing.
    shown = { label: 'value', tone: 'value', look: 'none', wrong: true, lines: [] };
  } else {
    shown = { ...literalBlock(value), tone: 'value', wrong: false, lines: [] };
  }
  // A field can be carried on from: adding a method after it makes it the start of a chain.
  const carryOn =
    value.kind === 'column' && !lists
      ? {
          label: `Add a method after ${value.column}`,
          onPress: (): void => {
            const id = nextId('chain');
            onChange({
              kind: 'chain',
              id,
              chain: { source: { kind: 'column', column: value.column }, methods: [] },
            });
            fresh.mark(`${id}:method`);
          },
        }
      : null;
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
      add={carryOn}
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
          placeholder={lists ? 'Search the lists' : 'Type a value, or search'}
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
  readonly test: Test;
  readonly frame: Frame;
  readonly onChange: (test: Test) => void;
}

/** Draws a comparison as three blocks: a field, a comparison and a value. */
function TestBlocks({ test, frame, onChange }: TestBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const catalogue = useCatalogue();
  const node = useNode();
  const prepared = usePreparedStep();
  const fieldHint = useFieldHint();
  const positions = usePositions();
  const column = findColumn(frame, test.column);
  const unset = test.column === '';
  // The value is the argument of the method the comparison is written as, which follows the
  // field: the engine says what fits it.
  const comparisonId = comparisonEntryId(test.comparison);
  const argument: ArgumentRequest | null =
    comparisonId === null || column === null || positions.length === 0
      ? null
      : {
          entry: comparisonId,
          parameter: 'other',
          place: { positions, after: { kind: 'value', dtype: column.dtype, shape: 'many' } },
        };
  const wrongSign = prepared?.wrong.has(wrongId({ kind: 'test', testId: test.id })) ?? false;

  // What each comparison is written as and means is the engine's, as the catalogue lists it.
  const comparisons = comparisonChoices(catalogue);
  const held = comparisonOption(catalogue, test.comparison);
  const compare = (comparison: Comparison): void => {
    const wasList = test.comparison === IS_IN;
    if (comparison === IS_IN && !wasList) {
      // Membership needs a list, so start from the first connected one.
      const list = node.inputs.find((input) => input.kind === 'list');
      onChange({ ...test, comparison, value: { kind: 'input', name: list?.name ?? '' } });
    } else if (comparison !== IS_IN && wasList) {
      onChange({ ...test, comparison, value: { kind: 'unset' } });
    } else {
      onChange({ ...test, comparison });
    }
  };

  return (
    <>
      <Hint lines={fieldHint(column)} code={false}>
        <Slot
          label={unset ? 'field' : test.column}
          tone={unset ? 'value' : 'column'}
          look={unset ? 'none' : 'plain'}
          wrong={column === null}
          openNow={fresh.id === test.id}
          describedAs="The field that is compared"
          placeholder="Search the fields"
          offers={(typed) => [
            counted(
              'Fields',
              fieldOffers(
                frame,
                typed,
                anyField((name) => {
                  onChange({ ...test, column: name });
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
        wrong={wrongSign}
        names
        describedAs={`Comparison: ${held.hint}`}
        placeholder="Search the comparisons"
        offers={(typed) => [
          titled(
            'Comparison',
            comparisons
              .filter((option) => matches(typed, option.entry, option.sign, option.hint))
              .map((option) =>
                offer(
                  option.sign,
                  'compare',
                  option.hint,
                  () => {
                    compare(option.entry);
                  },
                  { id: option.entry, look: 'compare' },
                ),
              ),
          ),
        ]}
      />
      <ValueSlot
        value={test.value}
        frame={frame}
        label={unset ? 'The value it is compared with' : `Value compared with ${test.column}`}
        lists={test.comparison === IS_IN}
        positions={positions.length === 0 ? null : positions}
        argument={argument}
        onChange={(value) => {
          onChange({ ...test, value });
        }}
      />
    </>
  );
}

/**
 * Returns `group` without the item called `id`. A group left with one item
 * is that item, and a group left with none is nothing, so it goes too.
 */
function withoutItem(group: Group, id: string): Condition | null {
  const items = group.items.filter((item) => item.id !== id);
  if (items.length === 0) return null;
  const [only] = items;
  return items.length === 1 && only !== undefined ? only : { ...group, items };
}

/** Returns the word that joins the lines of a group: `and` where all must hold, `or` where any may. */
function joiner(group: Group): string {
  return group.every ? 'and' : 'or';
}

interface GroupWordProps {
  readonly group: Group;
  /** The colour of the word: that of the step it stands in, or a function's on a line. */
  readonly tone: Tone;
  readonly onChange: (group: Group) => void;
}

/** Draws the word that heads a group, which says whether all of its lines must hold or any may. */
function GroupWord({ group, tone, onChange }: GroupWordProps): React.JSX.Element {
  const choose = (every: boolean): (() => void) => {
    return () => {
      onChange({ ...group, every });
    };
  };
  return (
    <Slot
      label={group.every ? 'all of' : 'any of'}
      tone={tone}
      look="plain"
      names
      describedAs="How the lines below are taken together"
      placeholder="Search"
      offers={(typed) => [
        titled(
          'The lines below',
          [
            offer('all of', 'function', 'every line must hold', choose(true)),
            offer('any of', 'function', 'one line is enough', choose(false)),
          ].filter((row) => matches(typed, row.label, row.note)),
        ),
      ]}
    />
  );
}

interface GroupLinesProps {
  readonly group: Group;
  readonly frame: Frame;
  /**
   * Whether the group is a line of another list. Such a group gives way when
   * lines are removed: left with one line it becomes that line, and left with
   * none it goes. A condition's own group stays a group down to its last
   * line, and with none left the condition is empty.
   */
  readonly nested: boolean;
  /** Reports the group as it is now, the one item it has come down to, or `null` when none is left. */
  readonly onChange: (condition: Condition | null) => void;
}

/**
 * Offers what a line added to `group` can be: a comparison, a group of the
 * other kind with lines of its own, or a chain. `onAdd` is given the line and the id of
 * the comparison in it that is filled in first.
 */
function lineOffers(
  catalogue: Catalogue,
  group: Group,
  typed: string,
  onAdd: (item: Condition, first: string) => void,
): OfferGroup[] {
  return [
    titled(
      'Add a line',
      [
        offer(
          'field == value',
          'value',
          'a comparison',
          () => {
            const test = emptyTest(catalogue);
            onAdd(test, test.id);
          },
          { look: 'none', act: true },
        ),
        offer(
          group.every ? 'any of' : 'all of',
          'function',
          'a group with lines of its own',
          () => {
            const test = emptyTest(catalogue);
            onAdd(
              { kind: 'group', id: nextId('group'), every: !group.every, items: [test] },
              test.id,
            );
          },
          { act: true },
        ),
        chainOffer((chain) => {
          onAdd(chain, chain.id);
        }),
      ].filter((row) => matches(typed, row.label, row.note)),
    ),
  ];
}

/** Where a line is about to be added: its place in the group and the piece its list opens from. */
interface LineInsertion {
  readonly at: number;
  readonly anchor: HTMLElement;
}

/**
 * Draws the lines of a group: a comparison, a chain, or the word of a group
 * with lines of its own set in beneath it, on each. The word that names a
 * line, a comparison's sign or a group's word, ends its list with adding a
 * line after it and taking the line away.
 */
function GroupLines({ group, frame, nested, onChange }: GroupLinesProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const catalogue = useCatalogue();
  const [insertion, setInsertion] = useState<LineInsertion | null>(null);
  const closeList = useCallback(() => {
    setInsertion(null);
  }, []);
  const withItem = (next: Condition, id: string): void => {
    onChange({ ...group, items: group.items.map((item) => (item.id === id ? next : item)) });
  };
  const insert = (at: number, item: Condition, first: string): void => {
    const items = [...group.items];
    items.splice(at, 0, item);
    onChange({ ...group, items });
    fresh.mark(first);
  };
  return (
    <>
      {group.items.map((item, index) => {
        const without = (): void => {
          onChange(
            nested
              ? withoutItem(group, item.id)
              : { ...group, items: group.items.filter((other) => other.id !== item.id) },
          );
        };
        const remove = { label: 'Remove this line', onPress: without };
        const add = {
          label: `Add a line after this one, joined by ${joiner(group)}`,
          onPress: (anchor: HTMLElement): void => {
            setInsertion({ at: index + 1, anchor });
          },
        };
        if (item.kind === 'chain') {
          // A chain is a line of its own, and its start takes the line away.
          return (
            <Line key={item.id}>
              <ChainBlocks
                chainId={item.id}
                chain={item.chain}
                frame={frame}
                onRemove={without}
                onChange={(chain) => {
                  withItem({ ...item, chain }, item.id);
                }}
              />
            </Line>
          );
        }
        return (
          <Line key={item.id}>
            {item.kind === 'test' ? (
              <Piece remove={remove} add={add}>
                <TestBlocks
                  test={item}
                  frame={frame}
                  onChange={(test) => {
                    withItem(test, item.id);
                  }}
                />
              </Piece>
            ) : (
              // A group is its word, and its lines set in beneath it.
              <>
                <Piece remove={remove} add={add}>
                  <GroupWord
                    group={item}
                    tone="function"
                    onChange={(inner) => {
                      withItem(inner, item.id);
                    }}
                  />
                </Piece>
                <Lines>
                  <GroupLines
                    group={item}
                    frame={frame}
                    nested
                    onChange={(inner) => {
                      // A group with no line left goes, as a line that is removed does.
                      if (inner === null) without();
                      else withItem(inner, item.id);
                    }}
                  />
                </Lines>
              </>
            )}
          </Line>
        );
      })}
      {insertion !== null && (
        <Popover anchor={insertion.anchor} onDismiss={closeList}>
          <OfferList
            placeholder="Search"
            groups={(typed) =>
              lineOffers(catalogue, group, typed, (item, first) => {
                insert(insertion.at, item, first);
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

/** What a condition shows and reports. */
export interface ConditionBlocksProps {
  readonly condition: Condition;
  readonly frame: Frame;
  /** The positions that hold the condition: a filter's predicates, or a `when`'s. */
  readonly positions: readonly PositionView[];
  /** The colour of the group that heads the list: that of the step, or a function's under a `when`. */
  readonly tone: Tone;
  readonly onChange: (condition: Condition) => void;
}

/**
 * Draws a condition where it stands. One that holds something is the word
 * that says whether all of its lines must hold or any may, and the lines set
 * in beneath it. The word's list ends with adding a line and with taking the
 * whole condition away, and removing its last line takes it away too. A
 * condition that is a chain of its own is one line. A condition with nothing
 * in it is an empty slot, which offers how to start one again: as a list of
 * comparisons, or as a chain.
 */
export function ConditionBlocks(props: ConditionBlocksProps): React.JSX.Element {
  return (
    <PositionsContext.Provider value={props.positions}>
      <ConditionContent {...props} />
    </PositionsContext.Provider>
  );
}

function ConditionContent({
  condition,
  frame,
  tone,
  onChange,
}: ConditionBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const catalogue = useCatalogue();
  const [adding, setAdding] = useState<HTMLElement | null>(null);
  const closeList = useCallback(() => {
    setAdding(null);
  }, []);
  if (condition.kind === 'chain') {
    return (
      <Lines>
        <Line>
          <ChainBlocks
            chainId={condition.id}
            chain={condition.chain}
            frame={frame}
            onRemove={() => {
              onChange({ kind: 'group', id: nextId('group'), every: true, items: [] });
            }}
            onChange={(chain) => {
              onChange({ ...condition, chain });
            }}
          />
        </Line>
      </Lines>
    );
  }
  const group: Group =
    condition.kind === 'group'
      ? condition
      : { kind: 'group', id: `${condition.id}:list`, every: true, items: [condition] };

  if (group.items.length === 0) {
    const start = (every: boolean): (() => void) => {
      return () => {
        const test = emptyTest(catalogue);
        onChange({ ...group, every, items: [test] });
        fresh.mark(test.id);
      };
    };
    return (
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
              offer('all of', 'function', 'every line must hold', start(true), { act: true }),
              offer('any of', 'function', 'one line is enough', start(false), { act: true }),
            ].filter((row) => matches(typed, row.label, row.note)),
          ),
          titled(
            'A chain',
            [
              chainOffer((chain) => {
                onChange(chain);
                fresh.mark(chain.id);
              }),
            ].filter((row) => matches(typed, row.label, row.note)),
          ),
        ]}
      />
    );
  }

  return (
    <>
      <Piece
        remove={{
          label: 'Remove the condition',
          onPress: () => {
            onChange({ ...group, items: [] });
          },
        }}
        add={{ label: `Add a line to ${group.every ? 'all of' : 'any of'}`, onPress: setAdding }}
      >
        <GroupWord group={group} tone={tone} onChange={onChange} />
      </Piece>
      <Lines>
        <GroupLines
          group={group}
          frame={frame}
          nested={false}
          onChange={(next) => {
            if (next !== null) onChange(next);
          }}
        />
      </Lines>
      {adding !== null && (
        <Popover anchor={adding} onDismiss={closeList}>
          <OfferList
            placeholder="Search"
            groups={(typed) =>
              lineOffers(catalogue, group, typed, (item, first) => {
                onChange({ ...group, items: [item, ...group.items] });
                fresh.mark(first);
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
