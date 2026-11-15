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

import { ChainBlocks } from './Chain';
import { COMPARISON_SIGNS } from './comparisons';
import { inputLines } from './InputsStrip';
import { FreshContext, Line, Lines } from './Lists';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import type { Cell, Dtype, Frame } from './model/frame';
import { findColumn, keysFit } from './model/frame';
import { nextId } from './model/ids';
import { INPUTS, findInput } from './model/inputs';
import type { ChainCondition, Comparison, Condition, Group, Test, Value } from './model/source';
import { COMPARISONS, dtypesAgree } from './model/source';

const WHOLE_NUMBER = /^-?\d+$/;

/** What each comparison means, for someone who reads the symbol for the first time. */
const COMPARISON_WORDS: Readonly<Record<Comparison, string>> = {
  '==': 'equal to',
  '!=': 'not equal to',
  '>': 'greater than',
  '>=': 'at least',
  '<': 'less than',
  '<=': 'at most',
  is_in: 'one of a list',
};

/** Returns how a typed value is shown as a block. */
function literalBlock(value: Cell): { label: string; look: Look } {
  if (value === null) return { label: 'None', look: 'none' };
  return typeof value === 'number'
    ? { label: String(value), look: 'plain' }
    : { label: value, look: 'quoted' };
}

/** Returns a comparison with nothing picked yet: a field, equal to, a value. */
export function emptyTest(): Test {
  return {
    kind: 'test',
    id: nextId('test'),
    column: '',
    comparison: '==',
    value: { kind: 'unset' },
  };
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
  /** The dtype the value is compared with, when it is compared with one. */
  readonly expected: Dtype | null;
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
  expected,
  onChange,
}: ValueSlotProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const fieldHint = useFieldHint();
  const fits = lists ? keysFit : dtypesAgree;
  const reasonFor = (dtype: Dtype): string | null =>
    expected === null || fits(expected, dtype) ? null : 'a different dtype';
  const literal = (cell: Cell): (() => void) => {
    return () => {
      onChange({ kind: 'literal', value: cell });
    };
  };

  const offers = (typed: string): OfferGroup[] => {
    const connected = INPUTS.flatMap((input) =>
      input.kind === (lists ? 'list' : 'value') ? [input] : [],
    )
      .filter((input) => matches(typed, input.name))
      .map((input) =>
        offer(
          input.name,
          'input',
          lists ? `list of ${input.dtype}` : input.dtype,
          () => {
            onChange({ kind: 'input', name: input.name });
          },
          { reason: reasonFor(input.dtype) },
        ),
      );
    if (lists) return [counted('Inputs', connected)];

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
      { id: 'chain', look: 'dots', act: true },
    );
    const written: Offer[] = [];
    const text = typed.trim();
    if (WHOLE_NUMBER.test(text) && Number.isSafeInteger(Number(text))) {
      written.push(offer(text, 'value', 'a number', literal(Number(text)), { id: 'number' }));
    }
    if (typed !== '') {
      written.push(offer(typed, 'value', 'String', literal(typed), { id: 'text', look: 'quoted' }));
    }
    written.push(offer('None', 'value', 'no value', literal(null), { id: 'none', look: 'none' }));
    const fields = fieldOffers(frame, typed, {
      ...anyField((column) => {
        onChange({ kind: 'column', column });
      }),
      reasonFor,
    });
    // The groups go from the shortest to the longest, so a long one never hides a short one.
    return [
      titled(null, typed === '' ? [chain] : []),
      titled('Value', written),
      counted('Inputs', connected),
      counted('Fields', fields),
    ];
  };

  if (value.kind === 'chain') {
    // A chain is written where it stands. Its start takes the whole chain away.
    return (
      <ChainBlocks
        chainId={value.id}
        chain={value.chain}
        frame={frame}
        onRemove={() => {
          onChange({ kind: 'unset' });
        }}
        onChange={(chain) => {
          onChange({ kind: 'chain', id: value.id, chain });
        }}
      />
    );
  }

  let shown: { label: string; tone: Tone; look: Look; wrong: boolean; lines: string[] };
  if (value.kind === 'input') {
    const input = findInput(value.name);
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
    shown = { ...literalBlock(value.value), tone: 'value', wrong: false, lines: [] };
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
    <Piece remove={null} add={carryOn}>
      <Hint lines={shown.lines} code={false}>
        <Slot
          label={shown.label}
          tone={shown.tone}
          look={shown.look}
          wrong={shown.wrong}
          names
          describedAs={label}
          placeholder={lists ? 'Search the lists' : 'Type a value, or search'}
          offers={offers}
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
  const fieldHint = useFieldHint();
  const column = findColumn(frame, test.column);
  const expected = column?.dtype ?? null;
  const unset = test.column === '';

  const compare = (comparison: Comparison): void => {
    const wasList = test.comparison === 'is_in';
    if (comparison === 'is_in' && !wasList) {
      // Membership needs a list, so start from a connected one of the field's dtype.
      const found = INPUTS.flatMap((input) => (input.kind === 'list' ? [input] : []));
      const list =
        found.find((input) => expected !== null && keysFit(expected, input.dtype)) ?? found[0];
      onChange({ ...test, comparison, value: { kind: 'input', name: list?.name ?? '' } });
    } else if (comparison !== 'is_in' && wasList) {
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
        label={COMPARISON_SIGNS[test.comparison]}
        tone="compare"
        look="compare"
        names
        describedAs={`Comparison: ${COMPARISON_WORDS[test.comparison]}`}
        placeholder="Search the comparisons"
        offers={(typed) => [
          titled(
            'Comparison',
            COMPARISONS.filter((comparison) =>
              matches(
                typed,
                comparison,
                COMPARISON_SIGNS[comparison],
                COMPARISON_WORDS[comparison],
              ),
            ).map((comparison) =>
              offer(
                COMPARISON_SIGNS[comparison],
                'compare',
                COMPARISON_WORDS[comparison],
                () => {
                  compare(comparison);
                },
                { id: comparison, look: 'compare' },
              ),
            ),
          ),
        ]}
      />
      <ValueSlot
        value={test.value}
        frame={frame}
        label={unset ? 'The value it is compared with' : `Value compared with ${test.column}`}
        lists={test.comparison === 'is_in'}
        expected={expected}
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
            const test = emptyTest();
            onAdd(test, test.id);
          },
          { look: 'none', act: true },
        ),
        offer(
          group.every ? 'any of' : 'all of',
          'function',
          'a group with lines of its own',
          () => {
            const test = emptyTest();
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
              lineOffers(group, typed, (item, first) => {
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
export function ConditionBlocks({
  condition,
  frame,
  tone,
  onChange,
}: ConditionBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
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
        const test = emptyTest();
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
              lineOffers(group, typed, (item, first) => {
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
