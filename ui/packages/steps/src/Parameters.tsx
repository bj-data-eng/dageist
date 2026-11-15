/**
 * @file The drawing of every kind of parameter the catalogue has, so that a
 * new operation needs data and not a new look. Each kind is written as text
 * in the colour of what it names. One click opens one searched list, and what
 * is typed goes into that list's search box: a number or text for a value, a
 * path, a duration. What is still to be given is written in the colour of a
 * mistake as soon as it exists.
 *
 * A parameter holds one of its kind, an ordered list of them, or one for each
 * item of another parameter. A method or a step shows the parameters that are
 * set. The others are added from the list its name opens and taken away
 * again from the same list, which puts them back to their default.
 */

import { useContext } from 'react';

import type { ChoiceView, EntryView, KindView, ParameterView } from '@dageist/host';
import type { Entry, Offer, OfferGroup } from '@dageist/design';
import { Block, Hint, ManySlot, Piece, Slot, Word } from '@dageist/design';

import { FreshContext, Line, Lines } from './Lists';
import { ValueSlot } from './Values';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import { DTYPES, findColumn } from './model/frame';
import type { Dtype, Frame } from './model/frame';
import { INPUTS, findInput } from './model/inputs';
import type { Options, PairSetting, SetParameter, Setting } from './model/settings';
import {
  UNSET,
  atDefault,
  defaultSetting,
  emptyPair,
  isComplete,
  setParameters,
  starterSetting,
  withOption,
  withoutOption,
} from './model/settings';

const WHOLE_NUMBER = /^-?\d+$/;
const DECIMAL_NUMBER = /^-?\d+(\.\d+)?$/;
/** Polars' durations: whole amounts of units written one after another, such as `1mo` or `2d12h`. */
const DURATION = /^-?(\d+(ns|us|ms|s|m|h|d|w|mo|q|y|i))+$/;
const DATE = /^\d{4}-\d{2}-\d{2}/;

/** Returns what a parameter is for, for assistive technology: its name, and its label when that adds to it. */
function describe(parameter: ParameterView): string {
  return parameter.label === parameter.shown
    ? parameter.shown
    : `${parameter.shown}: ${parameter.label}`;
}

/** What a drawing is drawn against. */
export interface Surroundings {
  /** The rows the parameter's fields are picked from. */
  readonly frame: Frame;
  /** Returns the items of the parameter named `of`, for a parameter that holds one value for each. */
  readonly items: (of: string) => readonly string[];
}

/** Returns how `setting` is written as a word. */
function wordOfSetting(kind: KindView, setting: Setting): { label: string; quoted: boolean } {
  switch (setting.kind) {
    case 'flag':
      return { label: setting.value ? 'true' : 'false', quoted: false };
    case 'number':
      return { label: String(setting.value), quoted: false };
    case 'text':
      return { label: setting.value, quoted: true };
    case 'word': {
      // A choice is shown by the words the catalogue gives it.
      const shown =
        kind.kind === 'choice'
          ? kind.words.find((choice) => choice.word === setting.value)?.shown
          : undefined;
      return { label: shown ?? setting.value, quoted: false };
    }
    default:
      return { label: '…', quoted: false };
  }
}

interface DrawingProps<V> {
  /** What the drawing is for, for assistive technology. */
  readonly describedAs: string;
  /** Whether the word names the piece it stands in, so that its list ends with what can be done to it. */
  readonly names?: boolean | undefined;
  /** The id that, when it is what was just added, opens the list as soon as the drawing is shown. */
  readonly openId?: string | undefined;
  readonly value: V;
  readonly onChange: (value: NonNullable<V>) => void;
}

/** Returns whether the thing with `id` was just added. */
function useIsFresh(id: string | undefined): boolean {
  const fresh = useContext(FreshContext);
  return id !== undefined && fresh.id === id;
}

/** Draws a flag: true or false. */
export function FlagDrawing({
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<boolean | null>): React.JSX.Element {
  const choose = (flag: boolean): (() => void) => {
    return () => {
      onChange(flag);
    };
  };
  return (
    <Slot
      label={value === null ? 'true or false' : String(value)}
      tone="value"
      look={value === null ? 'none' : 'plain'}
      wrong={value === null}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder="Search"
      offers={(typed) => [
        titled(
          null,
          [
            offer('true', 'value', '', choose(true)),
            offer('false', 'value', '', choose(false)),
          ].filter((row) => matches(typed, row.label)),
        ),
      ]}
    />
  );
}

/** Draws a choice of one word of a closed set, in the words the catalogue gives. */
export function ChoiceDrawing({
  words,
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null> & { readonly words: readonly ChoiceView[] }): React.JSX.Element {
  const picked = words.find((choice) => choice.word === value);
  return (
    <Slot
      label={value === null ? 'choice' : (picked?.shown ?? value)}
      tone="value"
      look={value === null ? 'none' : 'plain'}
      wrong={value === null || picked === undefined}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder="Search"
      offers={(typed) => [
        titled(
          null,
          words
            .filter((choice) => matches(typed, choice.shown, choice.word))
            .map((choice) =>
              offer(
                choice.shown,
                'value',
                choice.word === choice.shown ? '' : choice.word,
                () => {
                  onChange(choice.word);
                },
                { id: choice.word },
              ),
            ),
        ),
      ]}
    />
  );
}

/** Draws one field of the rows. */
export function ColumnDrawing({
  frame,
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null> & { readonly frame: Frame }): React.JSX.Element {
  const fieldHint = useFieldHint();
  const column = value === null ? null : findColumn(frame, value);
  return (
    <Hint lines={fieldHint(column)} code={false}>
      <Slot
        label={value ?? 'field'}
        tone={value === null ? 'value' : 'column'}
        look={value === null ? 'none' : 'plain'}
        wrong={value === null || column === null}
        names={names ?? false}
        openNow={useIsFresh(openId)}
        describedAs={describedAs}
        placeholder="Search the fields"
        offers={(typed) => [counted('Fields', fieldOffers(frame, typed, anyField(onChange)))]}
      />
    </Hint>
  );
}

/** What a list of fields shows and reports. */
export interface ColumnsDrawingProps {
  /** The rows the fields are picked from. */
  readonly frame: Frame;
  readonly names: readonly string[];
  /** What the slot says while it holds nothing. */
  readonly empty: string;
  readonly describedAs: string;
  /** Whether the list is wide enough for a control at the end of its rows. */
  readonly wide?: boolean;
  readonly openId?: string;
  /** Returns why a field of `dtype` cannot be picked, or `null` when it can. */
  readonly reasonFor?: (dtype: Dtype) => string | null;
  readonly onChange: (names: readonly string[]) => void;
}

function noReason(): null {
  return null;
}

/**
 * Draws several fields of the rows, in the order they are picked, folded to
 * the room on the line with a count of the rest.
 */
export function ColumnsDrawing({
  frame,
  names,
  empty,
  describedAs,
  wide = false,
  openId,
  reasonFor = noReason,
  onChange,
}: ColumnsDrawingProps): React.JSX.Element {
  const entries: Entry[] = names.map((name) => ({
    id: name,
    wrong: findColumn(frame, name) === null,
    blocks: [{ label: name, tone: 'column', look: 'plain' }],
  }));
  return (
    <ManySlot
      entries={entries}
      empty={empty}
      describedAs={describedAs}
      placeholder="Search the fields"
      wide={wide}
      openNow={useIsFresh(openId)}
      offers={(typed) => [
        counted(
          'Fields',
          fieldOffers(frame, typed, {
            reasonFor,
            chosen: names,
            renderExtra: () => null,
            onPick: (name) => {
              onChange(
                names.includes(name) ? names.filter((other) => other !== name) : [...names, name],
              );
            },
          }),
        ),
      ]}
    />
  );
}

/** Draws a table connected to the node. */
export function TableDrawing({
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null>): React.JSX.Element {
  const table = value === null ? null : findInput(value);
  const found = table?.kind === 'table' ? table : null;
  const lines =
    found === null
      ? []
      : [
          `${found.name}: ${String(found.frame[0]?.cells.length ?? 0)} rows`,
          ...found.frame.map((column) => `${column.name} · ${column.dtype}`),
        ];
  return (
    <Hint lines={lines} code>
      <Slot
        label={value ?? 'table'}
        tone={value === null ? 'value' : 'table'}
        look={value === null ? 'none' : 'plain'}
        wrong={value === null || found === null}
        names={names ?? false}
        openNow={useIsFresh(openId)}
        describedAs={describedAs}
        placeholder="Search the tables"
        offers={(typed) => [
          counted(
            'Tables',
            INPUTS.flatMap((input) => (input.kind === 'table' ? [input] : []))
              .filter((input) => matches(typed, input.name, input.from))
              .map((input) =>
                offer(
                  input.name,
                  'table',
                  `${String(input.frame[0]?.cells.length ?? 0)} × ${String(input.frame.length)}`,
                  () => {
                    onChange(input.name);
                  },
                ),
              ),
          ),
        ]}
      />
    </Hint>
  );
}

/** Draws a Polars dtype, from the dtypes the storyboard knows. */
export function DtypeDrawing({
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null>): React.JSX.Element {
  return (
    <Slot
      label={value ?? 'dtype'}
      tone="value"
      look={value === null ? 'none' : 'plain'}
      wrong={value === null}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder="Search the dtypes"
      offers={(typed) => [
        counted(
          'Dtypes',
          DTYPES.filter((dtype) => matches(typed, dtype)).map((dtype) =>
            offer(dtype, 'value', '', () => {
              onChange(dtype);
            }),
          ),
        ),
      ]}
    />
  );
}

/** What is typed into a literal: a number of a stated type, text, a duration, a path or a date. */
export type LiteralType = 'integer' | 'float' | 'string' | 'date' | 'any' | 'duration' | 'path';

/** What a literal holds. */
export type Literal =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'text'; readonly value: string };

/** What the search box says for each type of literal. */
const LITERAL_PLACEHOLDERS: Readonly<Record<LiteralType, string>> = {
  integer: 'Type a whole number',
  float: 'Type a number',
  string: 'Type text',
  date: 'Type a date, such as 2026-03-10',
  any: 'Type a number or text',
  duration: 'Type a duration, such as 1mo',
  path: 'Type a path',
};

/** What is waiting to be typed, for each type of literal. */
const LITERAL_WAITING: Readonly<Record<LiteralType, string>> = {
  integer: 'whole number',
  float: 'number',
  string: 'text',
  date: 'date',
  any: 'number or text',
  duration: 'duration',
  path: 'path',
};

/** What a row that offers typed text says it is, for each type of literal. */
const LITERAL_NOTES: Readonly<Record<LiteralType, string>> = {
  integer: 'a number',
  float: 'a number',
  string: 'String',
  date: 'a date',
  any: 'String',
  duration: 'a duration',
  path: 'a path',
};

/** Returns the rows that offer what was typed as a literal of `type`, with why one is not taken. */
function literalOffers(
  type: LiteralType,
  typed: string,
  onPick: (literal: Literal) => void,
): Offer[] {
  const rows: Offer[] = [];
  const text = typed.trim();
  const numeric = type === 'integer' ? WHOLE_NUMBER : DECIMAL_NUMBER;
  if (
    (type === 'integer' || type === 'float' || type === 'any') &&
    numeric.test(text) &&
    Number.isFinite(Number(text))
  ) {
    rows.push(
      offer(
        text,
        'value',
        type === 'integer' || WHOLE_NUMBER.test(text) ? 'a number' : 'a decimal number',
        () => {
          onPick({ kind: 'number', value: Number(text) });
        },
        { id: 'number' },
      ),
    );
  }
  if (type !== 'integer' && type !== 'float' && typed !== '') {
    let reason: string | null = null;
    if (type === 'duration' && !DURATION.test(text)) reason = 'not a duration such as 1mo';
    if (type === 'date' && !DATE.test(text)) reason = 'not a date such as 2026-03-10';
    const note = LITERAL_NOTES[type];
    rows.push(
      offer(
        typed,
        'value',
        note,
        () => {
          onPick({ kind: 'text', value: typed });
        },
        { id: 'text', look: 'quoted', reason },
      ),
    );
  }
  if (rows.length === 0 && typed === '') {
    // Nothing is typed yet, so the list says what goes in the search box.
    rows.push(
      offer(LITERAL_WAITING[type], 'value', '', () => undefined, {
        id: 'type',
        look: 'none',
        reason: 'type it in the search box',
      }),
    );
  }
  return rows;
}

/** Draws something typed into the search box of its list: a value, a duration or a path. */
export function LiteralDrawing({
  type,
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<Literal | null> & { readonly type: LiteralType }): React.JSX.Element {
  const placeholder = LITERAL_PLACEHOLDERS[type];
  const waiting = type === 'duration' || type === 'path' ? type : 'value';
  let shown: { label: string; look: 'plain' | 'quoted' | 'none' };
  if (value === null) shown = { label: waiting, look: 'none' };
  else if (value.kind === 'number') shown = { label: String(value.value), look: 'plain' };
  else shown = { label: value.value, look: 'quoted' };
  return (
    <Slot
      label={shown.label}
      tone="value"
      look={shown.look}
      wrong={value === null}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder={placeholder}
      offers={(typed) => [titled('Value', literalOffers(type, typed, onChange))]}
    />
  );
}

/** What drawing a setting of one kind needs to know. */
interface OneProps {
  readonly kind: KindView;
  readonly setting: Setting;
  readonly describedAs: string;
  readonly around: Surroundings;
  readonly names?: boolean | undefined;
  readonly openId?: string | undefined;
  /** Takes it away from where it stands, for a setting that is one of several. */
  readonly onRemove?: (() => void) | undefined;
  readonly onChange: (setting: Setting) => void;
}

/** Returns the literal a setting holds, or `null` when it holds none. */
function literalOf(setting: Setting): Literal | null {
  return setting.kind === 'number' || setting.kind === 'text' ? setting : null;
}

/** Returns the word a setting holds, or `null` when it holds none. */
function nameOf(setting: Setting): string | null {
  return setting.kind === 'word' ? setting.value : null;
}

/** Draws one setting of one kind. Every kind has one drawing; `pairs` and the shapes are drawn around these. */
function OneDrawing({
  kind,
  setting,
  describedAs,
  around,
  names,
  openId,
  onRemove,
  onChange,
}: OneProps): React.JSX.Element {
  const common = { describedAs, names, openId };
  const word = (name: string): void => {
    onChange({ kind: 'word', value: name });
  };
  switch (kind.kind) {
    case 'expression':
      return (
        <ValueSlot
          value={setting.kind === 'expression' ? setting.value : { kind: 'unset' }}
          frame={around.frame}
          label={describedAs}
          lists={false}
          expected={null}
          takes={kind.dtypes}
          openId={openId}
          onRemove={onRemove}
          onChange={(value) => {
            onChange({ kind: 'expression', value });
          }}
        />
      );
    case 'column':
      return (
        <ColumnDrawing {...common} frame={around.frame} value={nameOf(setting)} onChange={word} />
      );
    case 'value':
      return (
        <LiteralDrawing
          {...common}
          type={kind.type}
          value={literalOf(setting)}
          onChange={(literal) => {
            onChange(literal);
          }}
        />
      );
    case 'flag':
      return (
        <FlagDrawing
          {...common}
          value={setting.kind === 'flag' ? setting.value : null}
          onChange={(value) => {
            onChange({ kind: 'flag', value });
          }}
        />
      );
    case 'choice':
      return (
        <ChoiceDrawing {...common} words={kind.words} value={nameOf(setting)} onChange={word} />
      );
    case 'dtype':
      return <DtypeDrawing {...common} value={nameOf(setting)} onChange={word} />;
    case 'duration':
      return (
        <LiteralDrawing
          {...common}
          type="duration"
          value={literalOf(setting)}
          onChange={(literal) => {
            onChange(literal);
          }}
        />
      );
    case 'table':
      return <TableDrawing {...common} value={nameOf(setting)} onChange={word} />;
    case 'path':
      return (
        <LiteralDrawing
          {...common}
          type="path"
          value={literalOf(setting)}
          onChange={(literal) => {
            onChange(literal);
          }}
        />
      );
    case 'pairs':
      // Entries are one kind each, so a pair does not hold pairs.
      return <span className="font-sans text-xs text-wrong">pairs inside pairs are not drawn</span>;
    default:
      return kind satisfies never;
  }
}

/** What a list of settings shows and reports. */
interface ListProps {
  readonly parameter: ParameterView;
  readonly setting: Setting;
  readonly around: Surroundings;
  /** Tells what was just added from the rest. */
  readonly keyPrefix: string;
  readonly onChange: (setting: Setting) => void;
}

/** Draws several of one kind, in order: each can be taken away, and a faint word adds another. */
function ManyDrawing({
  parameter,
  setting,
  around,
  keyPrefix,
  onChange,
}: ListProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const { kind } = parameter;
  const items = setting.kind === 'list' ? setting.items : [UNSET];
  const set = (next: readonly Setting[]): void => {
    onChange({ kind: 'list', items: next });
  };
  if (kind.kind === 'column') {
    // Fields stand on the line and fold to the room there is.
    return (
      <ColumnsDrawing
        frame={around.frame}
        names={items.flatMap((item) => (item.kind === 'word' ? [item.value] : []))}
        empty="field"
        describedAs={describe(parameter)}
        openId={keyPrefix}
        onChange={(names) => {
          set(names.map((value) => ({ kind: 'word', value })));
        }}
      />
    );
  }
  return (
    <>
      {items.map((item, at) => (
        // The entries keep their places: only the last can be added and any can be taken away.
        <span key={at}>
          {at > 0 && <Word>,</Word>}
          <OneDrawing
            kind={kind}
            setting={item}
            describedAs={describe(parameter)}
            around={around}
            names
            openId={at === 0 ? keyPrefix : `${keyPrefix}:${String(at)}`}
            onRemove={() => {
              set(items.filter((_other, place) => place !== at));
            }}
            onChange={(next) => {
              set(items.map((other, place) => (place === at ? next : other)));
            }}
          />
        </span>
      ))}
      <Block
        label="+"
        tone="ink"
        look="plain"
        waits
        describedAs={`Add one to ${parameter.shown}`}
        onPress={() => {
          fresh.mark(`${keyPrefix}:${String(items.length)}`);
          set([...items, UNSET]);
        }}
      />
    </>
  );
}

/** Draws the entries of a `pairs` parameter: the key of each beside its value. */
function PairsDrawing({
  parameter,
  setting,
  around,
  keyPrefix,
  onChange,
}: ListProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const { kind } = parameter;
  if (kind.kind !== 'pairs') return <></>;
  const entries = setting.kind === 'pairs' ? setting.entries : [];
  const set = (next: readonly PairSetting[]): void => {
    onChange({ kind: 'pairs', entries: next });
  };
  // Keyword arguments are named; a dictionary maps one thing to another.
  const between = parameter.default.kind === 'empty' ? '=' : '→';
  return (
    <>
      {entries.map((entry) => {
        const describedAs = describe(parameter);
        return (
          <Piece
            key={entry.id}
            remove={{
              label: 'Remove this entry',
              onPress: () => {
                set(entries.filter((other) => other.id !== entry.id));
              },
            }}
            add={null}
          >
            <OneDrawing
              kind={kind.key}
              setting={entry.key}
              describedAs={`${describedAs}, the name`}
              around={around}
              names
              openId={`${keyPrefix}:${entry.id}`}
              onChange={(key) => {
                set(entries.map((other) => (other.id === entry.id ? { ...other, key } : other)));
              }}
            />
            <Word>{between}</Word>
            <OneDrawing
              kind={kind.value}
              setting={entry.value}
              describedAs={`${describedAs}, what it holds`}
              around={around}
              onChange={(value) => {
                set(entries.map((other) => (other.id === entry.id ? { ...other, value } : other)));
              }}
            />
          </Piece>
        );
      })}
      <Block
        label="+"
        tone="ink"
        look="plain"
        waits
        describedAs={`Add one to ${parameter.shown}`}
        onPress={() => {
          const added = emptyPair();
          fresh.mark(`${keyPrefix}:${added.id}`);
          set([...entries, added]);
        }}
      />
    </>
  );
}

/** The control at the end of a row that sets one item of a parameter that holds one value for each. */
function EachControl({
  parameter,
  item,
  value,
  onChange,
}: {
  readonly parameter: ParameterView;
  readonly item: string;
  readonly value: Setting | null;
  readonly onChange: (setting: Setting) => void;
}): React.JSX.Element {
  const { kind } = parameter;
  const label = `${parameter.shown} for ${item}`;
  if (kind.kind === 'flag' || kind.kind === 'choice') {
    // A setting with a few fixed answers is chosen from a list inside the list.
    const options =
      kind.kind === 'flag'
        ? [
            { word: 'true', shown: 'true' },
            { word: 'false', shown: 'false' },
          ]
        : kind.words;
    const current =
      value?.kind === 'flag' ? String(value.value) : value?.kind === 'word' ? value.value : '';
    return (
      <select
        aria-label={label}
        className="dg-given"
        value={current}
        onChange={(event) => {
          const chosen = event.currentTarget.value;
          onChange(
            kind.kind === 'flag'
              ? { kind: 'flag', value: chosen === 'true' }
              : { kind: 'word', value: chosen },
          );
        }}
      >
        {current === '' && <option value="">{'—'}</option>}
        {options.map((choice) => (
          <option key={choice.word} value={choice.word}>
            {choice.shown}
          </option>
        ))}
      </select>
    );
  }
  // A setting that can be anything is typed in a field inside the list.
  return (
    <input
      type="text"
      aria-label={label}
      className="dg-given"
      spellCheck={false}
      value={value === null ? '' : wordOfSetting(kind, value).label}
      onChange={(event) => {
        const typed = event.currentTarget.value;
        onChange(
          DECIMAL_NUMBER.test(typed)
            ? { kind: 'number', value: Number(typed) }
            : { kind: 'text', value: typed },
        );
      }}
    />
  );
}

/**
 * Draws a parameter that holds one value for each item of another, such as
 * the way each field of a sort is ordered. Its list sets one value for every
 * item, or a value for each.
 */
function EachDrawing({ parameter, setting, around, onChange }: ListProps): React.JSX.Element {
  const { kind, shape } = parameter;
  const items = shape.shape === 'perItem' ? around.items(shape.of) : [];
  const fallback = defaultSetting(parameter);
  const own = setting.kind === 'each' ? setting.items : {};
  const valueFor = (item: string): Setting | null => {
    if (Object.hasOwn(own, item)) return own[item] ?? null;
    return setting.kind === 'each' ? fallback : setting;
  };
  const setItem = (item: string, next: Setting): void => {
    // The items that had no setting of their own keep the one they shared.
    const shared = Object.fromEntries(
      items.flatMap((name) => {
        const held = valueFor(name);
        return held === null ? [] : [[name, held] as const];
      }),
    );
    onChange({ kind: 'each', items: { ...shared, [item]: next } });
  };
  const entries: Entry[] = items.map((item) => {
    const held = valueFor(item);
    const word = held === null ? { label: 'default', quoted: false } : wordOfSetting(kind, held);
    return {
      id: item,
      wrong: held !== null && !isComplete(held),
      blocks: [
        { label: item, tone: 'column', look: 'plain' },
        { label: word.label, tone: 'value', look: word.quoted ? 'quoted' : 'plain' },
      ],
    };
  });
  const all: Offer[] =
    kind.kind === 'flag'
      ? [true, false].map((flag) =>
          offer(
            String(flag),
            'value',
            '',
            () => {
              onChange({ kind: 'flag', value: flag });
            },
            { id: `all:${String(flag)}` },
          ),
        )
      : kind.kind === 'choice'
        ? kind.words.map((choice) =>
            offer(
              choice.shown,
              'value',
              '',
              () => {
                onChange({ kind: 'word', value: choice.word });
              },
              { id: `all:${choice.word}` },
            ),
          )
        : [];
  return (
    <ManySlot
      entries={entries}
      empty="no fields"
      describedAs={describe(parameter)}
      placeholder="Search"
      wide
      offers={(typed): OfferGroup[] => [
        titled(
          `${parameter.shown} for every one`,
          all.filter((row) => matches(typed, row.label)),
        ),
        counted(
          'One for each',
          items
            .filter((item) => matches(typed, item))
            .map((item) => {
              const held = valueFor(item);
              return offer(
                item,
                'column',
                held === null ? 'default' : wordOfSetting(kind, held).label,
                () => {
                  // A flag turns over when its row is pressed; anything else is set in the control.
                  if (held?.kind === 'flag') setItem(item, { kind: 'flag', value: !held.value });
                },
                {
                  id: `each:${item}`,
                  act: true,
                  extra: (
                    <EachControl
                      parameter={parameter}
                      item={item}
                      value={held}
                      onChange={(next) => {
                        setItem(item, next);
                      }}
                    />
                  ),
                },
              );
            }),
        ),
      ]}
    />
  );
}

/** What a parameter shows and reports. */
export interface ParameterDrawingProps {
  readonly parameter: ParameterView;
  readonly setting: Setting;
  readonly around: Surroundings;
  /** Tells what was just added from the rest, such as the step's id and the parameter's name. */
  readonly keyPrefix: string;
  readonly onChange: (setting: Setting) => void;
}

/**
 * Draws a parameter by its kind and shape: the kind's own drawing for one,
 * a list of them for many, or a value for each item of the parameter it
 * follows.
 */
export function ParameterDrawing({
  parameter,
  setting,
  around,
  keyPrefix,
  onChange,
}: ParameterDrawingProps): React.JSX.Element {
  const { kind, shape } = parameter;
  const notes = [`${parameter.shown} · ${parameter.label}`];
  let drawing: React.JSX.Element;
  if (shape.shape === 'perItem') {
    drawing = (
      <EachDrawing
        parameter={parameter}
        setting={setting}
        around={around}
        keyPrefix={keyPrefix}
        onChange={onChange}
      />
    );
  } else if (kind.kind === 'pairs') {
    drawing = (
      <PairsDrawing
        parameter={parameter}
        setting={setting}
        around={around}
        keyPrefix={keyPrefix}
        onChange={onChange}
      />
    );
  } else if (shape.shape === 'many') {
    drawing = (
      <ManyDrawing
        parameter={parameter}
        setting={setting}
        around={around}
        keyPrefix={keyPrefix}
        onChange={onChange}
      />
    );
  } else {
    drawing = (
      <OneDrawing
        kind={kind}
        setting={setting}
        describedAs={describe(parameter)}
        around={around}
        openId={keyPrefix}
        onChange={onChange}
      />
    );
  }
  return (
    <Hint lines={notes} code={false}>
      {drawing}
    </Hint>
  );
}

/** Lists the parameters that can be added, with those that are set ticked: picking one sets it or takes it away. */
export function optionOffers(
  parameters: readonly ParameterView[],
  options: Options,
  typed: string,
  onChange: (options: Options, added: string | null) => void,
): OfferGroup {
  return counted(
    'Options',
    parameters
      .filter((parameter) => matches(typed, parameter.shown, parameter.label, parameter.name))
      .map((parameter) => {
        const set = Object.hasOwn(options, parameter.name);
        return offer(
          parameter.shown,
          'ink',
          parameter.label,
          () => {
            if (set) {
              onChange(withoutOption(options, parameter.name), null);
            } else {
              onChange(
                withOption(options, parameter.name, starterSetting(parameter)),
                parameter.name,
              );
            }
          },
          { id: `option:${parameter.name}`, ticked: set },
        );
      }),
  );
}

/** What the parameters that are set show and report. */
export interface OptionRowsProps {
  /** Tells what was just added from the rest: the id of the method or step. */
  readonly ownerId: string;
  readonly entry: EntryView | null;
  /** The parameters that may be added: the entry's, less those the owner draws itself. */
  readonly parameters: readonly ParameterView[];
  readonly options: Options;
  readonly around: Surroundings;
  /** The names of the parameters that are set and that the preview does not apply. */
  readonly skipped: readonly string[];
  /** Whether each parameter stands on a line of its own beneath the owner, or all follow it on its line. */
  readonly layout: 'inline' | 'lines';
  readonly onChange: (options: Options) => void;
}

/** Draws every parameter of an owner that is set, with a word that says what the preview leaves out. */
export function OptionRows({
  ownerId,
  entry,
  parameters,
  options,
  around,
  skipped,
  layout,
  onChange,
}: OptionRowsProps): React.JSX.Element | null {
  if (entry === null) return null;
  const drawn: SetParameter[] = setParameters(entry, options).filter(({ parameter }) =>
    parameters.some((offered) => offered.name === parameter.name),
  );
  const rows = drawn.map(({ parameter, setting }) => (
    <Row key={parameter.name} layout={layout}>
      <Word>{parameter.shown}</Word>
      <ParameterDrawing
        parameter={parameter}
        setting={setting}
        around={around}
        keyPrefix={`${ownerId}:${parameter.name}`}
        onChange={(next) => {
          onChange(withOption(options, parameter.name, next));
        }}
      />
      {isComplete(setting) && atDefault(parameter, setting) && (
        <span className="font-sans text-xs text-muted">same as the default</span>
      )}
    </Row>
  ));
  const note =
    skipped.length === 0 ? null : (
      <Row layout={layout}>
        <span className="font-sans text-xs text-muted">
          {`The preview leaves out ${skipped.join(', ')}.`}
        </span>
      </Row>
    );
  if (drawn.length === 0) return null;
  return layout === 'lines' ? (
    <Lines>
      {rows}
      {note}
    </Lines>
  ) : (
    <>
      {rows}
      {note}
    </>
  );
}

function Row({
  layout,
  children,
}: {
  readonly layout: 'inline' | 'lines';
  readonly children: React.ReactNode;
}): React.JSX.Element {
  return layout === 'lines' ? <Line>{children}</Line> : <>{children}</>;
}
