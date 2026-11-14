/**
 * @file Values and the conditions made of them. A value is a field, a
 * connected input, something typed, or a chain of its own. A comparison is a
 * field, a comparison and a value as three blocks. A condition is one
 * comparison where it stands, or a list of them under its row where all must
 * hold or any may, with a group as a list inside the list.
 */

import { useContext } from 'react';

import type { Look, Offer, OfferGroup, Tone } from '@dageist/design';
import { Block, Hint, Slot } from '@dageist/design';

import { ChainBlocks } from './Chain';
import { inputLines } from './InputsStrip';
import { Dots, FreshContext, Line, OwnedLines } from './Lists';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import type { Cell, Dtype, Frame } from './model/frame';
import { distinctValues, findColumn, keysFit } from './model/frame';
import { nextId } from './model/ids';
import { INPUTS, findInput } from './model/inputs';
import type { Comparison, Condition, Group, Test, Value } from './model/source';
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

/** What a value slot shows and reports. */
export interface ValueSlotProps {
  readonly value: Value;
  readonly frame: Frame;
  /** The field whose values are worth offering by name, such as the one compared with. */
  readonly suggestFrom: string | null;
  /** What the value is for, for assistive technology. */
  readonly label: string;
  /** Whether the slot takes a connected list, as `is_in` does, instead of a single value. */
  readonly lists: boolean;
  /** The dtype the value is compared with, when it is compared with one. */
  readonly expected: Dtype | null;
  readonly onChange: (value: Value) => void;
}

/**
 * Draws a value as a block in the colour of what it is, which opens the list
 * it is picked from: a chain of its own, something typed, a value of the
 * compared field, a connected input, or a field. A chain is three dots, with
 * its own list under the row.
 */
export function ValueSlot({
  value,
  frame,
  suggestFrom,
  label,
  lists,
  expected,
  onChange,
}: ValueSlotProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
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
    const known =
      suggestFrom === null
        ? []
        : distinctValues(frame, suggestFrom)
            .filter((cell) => cell !== null && matches(typed, String(cell)))
            .map((cell) => {
              const block = literalBlock(cell);
              return offer(block.label, 'value', `in ${suggestFrom}`, literal(cell), {
                id: `known:${block.label}`,
                look: block.look,
              });
            });
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
      counted(suggestFrom === null ? 'Values' : `Values of ${suggestFrom}`, known),
      counted('Fields', fields),
    ];
  };

  if (value.kind === 'chain') {
    return (
      <Dots id={value.id} label={`${label}: a chain of its own`} word={null}>
        <Line
          waits={false}
          onRemove={() => {
            onChange({ kind: 'unset' });
          }}
        >
          <span className="dg-flow">
            <ChainBlocks
              chainId={value.id}
              chain={value.chain}
              frame={frame}
              opensNew
              onChange={(chain) => {
                onChange({ kind: 'chain', id: value.id, chain });
              }}
            />
          </span>
        </Line>
      </Dots>
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
      lines: column === null ? [] : [`${column.name} · ${column.dtype}`],
    };
  } else if (value.kind === 'unset') {
    // Nothing is picked yet, so the slot says what goes there.
    shown = { label: 'value', tone: 'value', look: 'none', wrong: false, lines: [] };
  } else {
    shown = { ...literalBlock(value.value), tone: 'value', wrong: false, lines: [] };
  }
  return (
    <Hint lines={shown.lines} code={false}>
      <Slot
        label={shown.label}
        tone={shown.tone}
        look={shown.look}
        wrong={shown.wrong}
        describedAs={label}
        placeholder={lists ? 'Search the lists' : 'Type a value, or search'}
        offers={offers}
      />
    </Hint>
  );
}

/** What a comparison shows and reports. */
export interface TestBlocksProps {
  readonly test: Test;
  readonly frame: Frame;
  readonly onChange: (test: Test) => void;
}

/** Draws a comparison as three blocks: a field, a comparison and a value. */
export function TestBlocks({ test, frame, onChange }: TestBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
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
      <Hint lines={column === null ? [] : [`${column.name} · ${column.dtype}`]} code={false}>
        <Slot
          label={unset ? 'field' : test.column}
          tone={unset ? 'value' : 'column'}
          look={unset ? 'none' : 'plain'}
          wrong={!unset && column === null}
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
        label={test.comparison}
        tone="compare"
        look="compare"
        describedAs={`Comparison: ${COMPARISON_WORDS[test.comparison]}`}
        placeholder="Search the comparisons"
        offers={(typed) => [
          titled(
            'Comparison',
            COMPARISONS.filter((comparison) =>
              matches(typed, comparison, COMPARISON_WORDS[comparison]),
            ).map((comparison) =>
              offer(
                comparison,
                'compare',
                COMPARISON_WORDS[comparison],
                () => {
                  compare(comparison);
                },
                { look: 'compare' },
              ),
            ),
          ),
        ]}
      />
      <ValueSlot
        value={test.value}
        frame={frame}
        suggestFrom={unset ? null : test.column}
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
  /** Reports the group as it is now, the one item it has come down to, or `null` when none is left. */
  readonly onChange: (condition: Condition | null) => void;
}

/**
 * Draws the lines of a group: a comparison, or a group with lines of its
 * own, on each. A faint word waits at the end to add the next line.
 */
function GroupLines({ group, frame, onChange }: GroupLinesProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const withItem = (next: Condition, id: string): void => {
    onChange({ ...group, items: group.items.map((item) => (item.id === id ? next : item)) });
  };
  const add = (item: Condition, first: string): void => {
    onChange({ ...group, items: [...group.items, item] });
    fresh.mark(first);
  };
  return (
    <>
      {group.items.map((item) => (
        <Line
          key={item.id}
          waits={false}
          onRemove={() => {
            onChange(withoutItem(group, item.id));
          }}
        >
          {item.kind === 'test' ? (
            <span className="dg-run">
              <TestBlocks
                test={item}
                frame={frame}
                onChange={(test) => {
                  withItem(test, item.id);
                }}
              />
            </span>
          ) : (
            <>
              <GroupWord
                group={item}
                tone="function"
                onChange={(inner) => {
                  withItem(inner, item.id);
                }}
              />
              <OwnedLines>
                <GroupLines
                  group={item}
                  frame={frame}
                  onChange={(inner) => {
                    // A group with no line left goes, as a line that is removed does.
                    if (inner === null) onChange(withoutItem(group, item.id));
                    else withItem(inner, item.id);
                  }}
                />
              </OwnedLines>
            </>
          )}
        </Line>
      ))}
      <Line waits onRemove={null}>
        <Slot
          label={joiner(group)}
          tone="function"
          look="plain"
          waits
          describedAs="Add a line to this list"
          placeholder="Search"
          offers={(typed) => [
            titled(
              'Add a line',
              [
                offer(
                  'field == value',
                  'value',
                  'a comparison',
                  () => {
                    const test = emptyTest();
                    add(test, test.id);
                  },
                  { look: 'none', act: true },
                ),
                offer(
                  group.every ? 'any of' : 'all of',
                  'function',
                  'a group with lines of its own',
                  () => {
                    const test = emptyTest();
                    add(
                      { kind: 'group', id: nextId('group'), every: !group.every, items: [test] },
                      test.id,
                    );
                  },
                  { act: true },
                ),
              ].filter((row) => matches(typed, row.label, row.note)),
            ),
          ]}
        />
      </Line>
    </>
  );
}

/** What a condition shows and reports. */
export interface ConditionBlocksProps {
  readonly condition: Condition;
  readonly frame: Frame;
  /** The colour of the group's word: that of the step, or a function's in a case. */
  readonly tone: Tone;
  /**
   * Whether the condition stands on its own, as in a case, and not inside a
   * piece: one comparison is then a run of blocks by itself.
   */
  readonly alone: boolean;
  readonly onChange: (condition: Condition) => void;
}

/**
 * Draws a condition where it stands. One comparison is three blocks, with a
 * faint `and` after it: a row holds one comparison, and the word moves the
 * condition into a list under the row, where the next one is added. A list
 * is the word that heads it, and its lines under the row.
 */
export function ConditionBlocks({
  condition,
  frame,
  tone,
  alone,
  onChange,
}: ConditionBlocksProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  if (condition.kind === 'test') {
    const test = <TestBlocks test={condition} frame={frame} onChange={onChange} />;
    return (
      <>
        {alone ? <span className="dg-run">{test}</span> : test}
        <Block
          label="and"
          tone={tone}
          look="plain"
          waits
          describedAs="Add a comparison: the condition becomes a list"
          onPress={() => {
            const next = emptyTest();
            onChange({ kind: 'group', id: nextId('group'), every: true, items: [condition, next] });
            fresh.mark(next.id);
          }}
        />
      </>
    );
  }
  return (
    <>
      <GroupWord group={condition} tone={tone} onChange={onChange} />
      <OwnedLines>
        <GroupLines
          group={condition}
          frame={frame}
          onChange={(next) => {
            // The condition of a step or a case always has something in it. A list that has
            // come down to one line is that line, back on its row.
            let settled = next;
            while (settled?.kind === 'group' && settled.items.length === 1) {
              settled = settled.items[0] ?? null;
            }
            if (settled !== null) onChange(settled);
          }}
        />
      </OwnedLines>
    </>
  );
}
