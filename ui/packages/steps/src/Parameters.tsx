/**
 * @file The drawing of every kind of parameter the catalogue has, so that a new operation needs
 * data and not a new look. Each kind is written as text in the colour of what it names. One click
 * opens one searched list, and what is typed goes into that list's search box: a number or text
 * for a value, a path, a duration. What is still to be given is written in the colour of a mistake
 * as soon as it exists.
 *
 * A parameter holds one of its kind, an ordered list of them, or one for each item of another
 * parameter. A method or a step shows the parameters it was given. The others are added from the
 * list its name opens and taken away again from the same list, which puts them back to their
 * default. What is drawn is what the document holds; each pick is sent as an edit, and the window
 * draws the document the host answers with. What has no value yet, such as an option that needs
 * text typed in, is held as a draft until it has one.
 */

import { useContext } from 'react';

import type {
  ArgumentRequest,
  ArgumentView,
  CallView,
  ChoiceView,
  EntryView,
  FieldView,
  KindView,
  LiteralView,
  NoneView,
  OfferPlaceView,
  ParameterView,
  TypedKind,
  ValueRequest,
} from '@dageist/host';
import type { Entry, Offer, OfferGroup } from '@dageist/design';
import { Block, Hint, ManySlot, Piece, Slot, Word } from '@dageist/design';

import type { Asked } from './ask';
import { useReadTyped } from './ask';
import { useDrafts, draftKey } from './drafts';
import { useStepAt, useStepEdits } from './editor';
import { FreshContext, Line, Lines } from './Lists';
import { useCatalogue, useNode } from './EngineProvider';
import { ValueSlot } from './Values';
import { readingRows } from './engineLists';
import { anyField, counted, fieldOffers, matches, offer, titled } from './offers';
import { useFieldHint } from './origins';
import { useListState, usePositions } from './positions';
import { usePreparedStep } from './engine';
import type { LiteralType } from './model/controls';
import {
  controlOf,
  defaultWordOf,
  oneControlOf,
  pairsJoiner,
  startsWithEngineWord,
} from './model/controls';
import type { Path } from './model/doc';
import { expressionOf, itemSegment, keySegment, pathKey, under, valueSegment } from './model/doc';
import { parameterSegment } from './model/doc';
import { handlingOf } from './model/edits';
import { findColumn } from './model/frame';
import type { Dtype, Frame } from './model/frame';
import { findInput } from './model/inputs';
import type { PreparedOption, PreparedReading } from './model/offers';
import { placeRequest, within } from './model/place';
import type { Shown } from './model/values';
import { shownOf } from './model/values';
import { actionsOf, noAddition } from './model/words';
import {
  HOLE,
  canBeLeftUnpicked,
  flagArgument,
  listArgument,
  literalArgument,
  wordArgument,
} from './model/requests';

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
  /**
   * The method whose parameters these are, and where the engine reported it stands, when they
   * are a method's: an expression one of them holds is the argument of that method.
   */
  readonly method?:
    | {
        readonly entry: string;
        readonly place: OfferPlaceView | null;
        /** The fields of the struct the method follows, where the engine knows them. */
        readonly fields: readonly FieldView[] | null;
      }
    | undefined;
}

/** The key a fresh thing at `path` of the step being drawn is known by, which a drawing opens at. */
export function freshKey(stepId: string, path: Path): string {
  return `${stepId}:${pathKey(path)}`;
}

/** Returns the one word a column, choice, dtype or table argument holds, or `null`. */
function wordOf(argument: ArgumentView | undefined): string | null {
  switch (argument?.kind) {
    case 'columns':
      return argument.names[0] ?? null;
    case 'choice':
      return argument.word;
    case 'dtype':
      return argument.name;
    case 'table':
      return argument.name;
    default:
      return null;
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

/**
 * Draws the name of a field of the struct the method follows, among the fields the engine says the
 * struct has, each with its dtype. Where the engine does not know them, none is offered.
 */
export function FieldDrawing({
  fields,
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null> & {
  readonly fields: readonly FieldView[] | null;
}): React.JSX.Element {
  const known = fields ?? [];
  return (
    <Slot
      label={value ?? 'field'}
      tone="column"
      look={value === null ? 'none' : 'plain'}
      wrong={value === null || (fields !== null && !known.some((field) => field.name === value))}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder="Search"
      offers={(typed) => [
        titled(
          null,
          known
            .filter((field) => matches(typed, field.name, field.dtype))
            .map((field) =>
              offer(
                field.name,
                'column',
                field.dtype,
                () => {
                  onChange(field.name);
                },
                { id: field.name },
              ),
            ),
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
}: DrawingProps<string | null> & {
  readonly frame: Frame;
}): React.JSX.Element {
  const fieldHint = useFieldHint();
  const column = value === null ? null : findColumn(frame, value);
  const fields = (typed: string): OfferGroup => {
    return counted('Fields', fieldOffers(frame, typed, anyField(onChange)));
  };
  return (
    <Hint lines={fieldHint(column)} code={false}>
      <Slot
        label={value === null || value === '' ? 'field' : value}
        tone={value === null || value === '' ? 'value' : 'column'}
        look={value === null || value === '' ? 'none' : 'plain'}
        wrong={value === null || column === null}
        names={names ?? false}
        openNow={useIsFresh(openId)}
        describedAs={describedAs}
        placeholder="Search the fields"
        offers={(typed) => [fields(typed)]}
      />
    </Hint>
  );
}

/** What a list of fields shows and reports. */
interface ColumnsDrawingProps {
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
  /** Picks the field `name`, which the list has not, or takes it away, which it has. */
  readonly onToggle: (name: string, held: boolean) => void;
}

function noReason(): null {
  return null;
}

/**
 * Draws several fields of the rows, in the order they are picked, folded to
 * the room on the line with a count of the rest.
 */
function ColumnsDrawing({
  frame,
  names,
  empty,
  describedAs,
  wide = false,
  openId,
  reasonFor = noReason,
  onToggle,
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
              onToggle(name, names.includes(name));
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
  const node = useNode();
  const table = value === null ? null : findInput(node, value);
  const found = table?.kind === 'table' ? table : null;
  const lines =
    found === null
      ? []
      : [
          `${found.name} · table`,
          ...found.columns.map((column) => `${column.name} · ${column.dtype}`),
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
            node.inputs
              .flatMap((input) => (input.kind === 'table' ? [input] : []))
              .filter((input) => matches(typed, input.name))
              .map((input) =>
                offer(input.name, 'table', `${String(input.columns.length)} columns`, () => {
                  onChange(input.name);
                }),
              ),
          ),
        ]}
      />
    </Hint>
  );
}

/** Draws a Polars dtype, from the dtypes the document names. */
export function DtypeDrawing({
  describedAs,
  names,
  openId,
  value,
  onChange,
}: DrawingProps<string | null>): React.JSX.Element {
  // The dtypes a document names by one word, which the engine lists.
  const { listedDtypes } = useCatalogue().view;
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
          listedDtypes
            .filter((dtype) => matches(typed, dtype))
            .map((dtype) =>
              offer(dtype, 'value', '', () => {
                onChange(dtype);
              }),
            ),
        ),
      ]}
    />
  );
}

/** Returns what the engine is asked a text is typed into, for a literal of `type`. */
function typedInto(type: LiteralType): TypedKind {
  switch (type) {
    case 'duration':
      return { kind: 'duration' };
    case 'pattern':
      return { kind: 'pattern' };
    case 'timeZone':
      return { kind: 'timeZone' };
    case 'path':
      return { kind: 'path' };
    default:
      return { kind: 'value', type };
  }
}

/** What the search box says for each type of literal. */
const LITERAL_PLACEHOLDERS: Readonly<Record<LiteralType, string>> = {
  integer: 'Type a whole number',
  float: 'Type a number',
  string: 'Type text',
  date: 'Type a date, such as 2026-03-10',
  any: 'Type a number or text',
  duration: 'Type a duration, such as 1mo',
  pattern: 'Type a pattern, such as [aeiou]',
  timeZone: 'Type a time zone, such as Europe/Berlin',
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
  pattern: 'pattern',
  timeZone: 'time zone',
  path: 'path',
};

/**
 * Returns the rows that offer what was typed as the engine read it for a
 * literal of `type`, with why one is not taken. Nothing is typed yet, so the
 * list says what goes in the search box.
 */
function literalOffers(
  type: LiteralType,
  typed: string,
  readings: Asked<readonly PreparedReading[]>,
  onPick: (literal: LiteralView) => void,
): Offer[] {
  if (typed === '') {
    return [
      offer(LITERAL_WAITING[type], 'value', '', () => undefined, {
        id: 'type',
        look: 'none',
        reason: 'type it in the search box',
      }),
    ];
  }
  return readingRows(typed, readings, onPick);
}

/** Draws something typed into the search box of its list: a value, a duration, a pattern or a path. */
export function LiteralDrawing({
  type,
  describedAs,
  names,
  openId,
  value,
  onChange,
}: Omit<DrawingProps<Shown | null>, 'onChange'> & {
  readonly type: LiteralType;
  readonly onChange: (literal: LiteralView) => void;
}): React.JSX.Element {
  const placeholder = LITERAL_PLACEHOLDERS[type];
  const waiting = type === 'duration' || type === 'pattern' || type === 'path' ? type : 'value';
  const list = useListState();
  // The engine reads what is typed, only while the list is open.
  const readings = useReadTyped(
    list.open ? { text: list.typed, typed: typedInto(type), slot: null } : null,
  );
  const shown: { label: string; look: 'plain' | 'quoted' | 'none' } =
    value === null
      ? { label: waiting, look: 'none' }
      : { label: value.shown, look: value.text ? 'quoted' : 'plain' };
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
      offers={(typed) => [titled('Value', literalOffers(type, typed, readings, onChange))]}
      onOpenChange={list.onOpenChange}
      onType={list.onType}
    />
  );
}

/**
 * Draws a time zone typed into the search box of its list, or, where the parameter may be given
 * none, the word `None`, which is no zone at all: it takes the zone off a moment.
 */
function TimeZoneDrawing({
  describedAs,
  names,
  openId,
  argument,
  none,
  onPick,
}: {
  readonly describedAs: string;
  readonly names?: boolean | undefined;
  readonly openId?: string | undefined;
  readonly argument: ArgumentView | undefined;
  readonly none: NoneView | null;
  readonly onPick: (request: ValueRequest) => void;
}): React.JSX.Element {
  const list = useListState();
  // The engine reads what is typed, only while the list is open.
  const readings = useReadTyped(
    list.open ? { text: list.typed, typed: typedInto('timeZone'), slot: null } : null,
  );
  const value = shownOf(argument);
  const isNone = none !== null && argument?.kind === 'timeZone' && argument.zone === null;
  let shown: { label: string; look: 'plain' | 'quoted' | 'none' };
  if (isNone) shown = { label: none.shown, look: 'plain' };
  else if (value === null) shown = { label: LITERAL_WAITING.timeZone, look: 'none' };
  else shown = { label: value.shown, look: 'quoted' };
  return (
    <Slot
      label={shown.label}
      tone="value"
      look={shown.look}
      wrong={value === null && !isNone}
      names={names ?? false}
      openNow={useIsFresh(openId)}
      describedAs={describedAs}
      placeholder={LITERAL_PLACEHOLDERS.timeZone}
      offers={(typed) => [
        titled(
          'Value',
          literalOffers('timeZone', typed, readings, (literal) => {
            const request = literalArgument({ kind: 'timeZone', none }, literal);
            if (request !== null) onPick(request);
          }),
        ),
        ...(none === null
          ? []
          : [
              titled('No time zone', [
                offer(none.shown, 'value', none.hint, () => {
                  onPick({ kind: 'timeZone', zone: null });
                }),
              ]),
            ]),
      ]}
      onOpenChange={list.onOpenChange}
      onType={list.onType}
    />
  );
}

/** What drawing an argument of one kind needs to know. */
interface OneProps {
  readonly kind: KindView;
  /** The parameter that holds the argument, which says where an expression it holds stands. */
  readonly parameter: ParameterView;
  /** What the document holds there, or `undefined` where nothing is yet. */
  readonly argument: ArgumentView | undefined;
  /** The way to the argument in the step, or `null` for one that is not in the document yet. */
  readonly path: Path | null;
  readonly describedAs: string;
  readonly around: Surroundings;
  readonly names?: boolean | undefined;
  readonly openId?: string | undefined;
  /** Takes it away from where it stands, for an argument that is one of several. */
  readonly onRemove?: (() => void) | undefined;
  /** Says what the person picked for it. */
  readonly onSet: (request: ValueRequest) => void;
}

/** Draws one argument of one kind, as the model says the kind is drawn. `pairs` and the shapes are drawn around these. */
function OneDrawing({
  kind,
  parameter,
  argument,
  path,
  describedAs,
  around,
  names,
  openId,
  onRemove,
  onSet,
}: OneProps): React.JSX.Element {
  const common = { describedAs, names, openId };
  const outer = usePositions();
  // An expression a parameter holds stands where its position says, or else where the chain
  // that holds the parameter stands, and is the argument of the method the parameter is of.
  const stands = within(outer, parameter.position);
  const asked: ArgumentRequest | null =
    around.method?.place == null
      ? null
      : {
          entry: around.method.entry,
          parameter: parameter.name,
          place: placeRequest(around.method.place),
        };
  const drawn = oneControlOf(kind);
  switch (drawn.control) {
    case 'expression':
      return (
        <ValueSlot
          expression={expressionOf(argument)}
          path={path}
          frame={around.frame}
          label={describedAs}
          positions={stands.length === 0 ? null : stands}
          argument={asked}
          openId={openId}
          onRemove={onRemove}
          onSet={onSet}
        />
      );
    case 'column':
      return (
        <ColumnDrawing
          {...common}
          frame={around.frame}
          value={wordOf(argument)}
          onChange={(name) => {
            onSet(wordArgument(kind, name));
          }}
        />
      );
    case 'field':
      return (
        <FieldDrawing
          {...common}
          fields={around.method?.fields ?? null}
          value={argument?.kind === 'field' ? argument.name : null}
          onChange={(name) => {
            onSet(wordArgument(kind, name));
          }}
        />
      );
    case 'literal':
      return (
        <LiteralDrawing
          {...common}
          type={drawn.type}
          value={shownOf(argument)}
          onChange={(literal) => {
            const request = literalArgument(kind, literal);
            if (request !== null) onSet(request);
          }}
        />
      );
    case 'flag':
      return (
        <FlagDrawing
          {...common}
          value={argument?.kind === 'flag' ? argument.value : null}
          onChange={(value) => {
            onSet(flagArgument(value));
          }}
        />
      );
    case 'choice':
      return (
        <ChoiceDrawing
          {...common}
          words={drawn.words}
          value={wordOf(argument)}
          onChange={(word) => {
            onSet(wordArgument(kind, word));
          }}
        />
      );
    case 'dtype':
      return (
        <DtypeDrawing
          {...common}
          value={wordOf(argument)}
          onChange={(word) => {
            onSet(wordArgument(kind, word));
          }}
        />
      );
    case 'timeZone':
      return <TimeZoneDrawing {...common} argument={argument} none={drawn.none} onPick={onSet} />;
    case 'table':
      return (
        <TableDrawing
          {...common}
          value={wordOf(argument)}
          onChange={(name) => {
            onSet(wordArgument(kind, name));
          }}
        />
      );
    case 'none':
      return <span className="font-sans text-xs text-wrong">{drawn.why}</span>;
    default:
      return drawn satisfies never;
  }
}

/** What a list or pairs shows and reports. */
interface ListProps {
  readonly parameter: ParameterView;
  /** What the document holds for the parameter, or `undefined` where it is not given. */
  readonly argument: ArgumentView | undefined;
  /** The way to the argument in the step. */
  readonly path: Path;
  readonly around: Surroundings;
}

/** Draws several of one kind, in order: each can be taken away, and a faint word adds another. */
function ManyDrawing({ parameter, argument, path, around }: ListProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const edits = useStepEdits();
  const step = useStepAt();
  const drafts = useDrafts();
  const { kind } = parameter;
  const items = argument?.kind === 'many' ? argument.items : [];
  const draftAt = draftKey('item', step.id, pathKey(path));
  const drafting = drafts.get(draftAt) !== undefined;
  if (controlOf(parameter).shape === 'columns' && kind.kind === 'column') {
    // Fields stand on the line and fold to the room there is.
    const names = items.flatMap((item) => {
      const name = wordOf(item);
      return name === null ? [] : [name];
    });
    return (
      <ColumnsDrawing
        frame={around.frame}
        names={names}
        empty="field"
        describedAs={describe(parameter)}
        openId={freshKey(step.id, path)}
        onToggle={(name, held) => {
          if (held) {
            void edits.removeItem(under(path, itemSegment(names.indexOf(name))));
          } else if (argument === undefined) {
            void edits.setArgument(path, listArgument([wordArgument(kind, name)]));
          } else {
            void edits.insertItem(path, items.length, wordArgument(kind, name));
          }
        }}
      />
    );
  }
  const open = (at: number): string => freshKey(step.id, under(path, itemSegment(at)));
  return (
    <>
      {items.map((item, at) => (
        // The entries keep their places: only the last can be added and any can be taken away.
        <span key={at}>
          {at > 0 && <Word>,</Word>}
          <OneDrawing
            kind={kind}
            parameter={parameter}
            argument={item}
            path={under(path, itemSegment(at))}
            describedAs={describe(parameter)}
            around={around}
            names
            openId={open(at)}
            onRemove={() => {
              void edits.removeItem(under(path, itemSegment(at)));
            }}
            onSet={(request) => {
              void edits.setArgument(under(path, itemSegment(at)), request);
            }}
          />
        </span>
      ))}
      {drafting && (
        // An item that has no value yet is not in the document: it waits here until it has one.
        <span>
          {items.length > 0 && <Word>,</Word>}
          <OneDrawing
            kind={kind}
            parameter={parameter}
            argument={undefined}
            path={null}
            describedAs={describe(parameter)}
            around={around}
            names
            openId={open(items.length)}
            onRemove={() => {
              drafts.clear(draftAt);
            }}
            onSet={(request) => {
              drafts.clear(draftAt);
              if (argument === undefined) void edits.setArgument(path, listArgument([request]));
              else void edits.insertItem(path, items.length, request);
            }}
          />
        </span>
      )}
      <Block
        label="+"
        tone="ink"
        look="plain"
        waits
        describedAs={`Add one to ${parameter.shown}`}
        onPress={() => {
          fresh.mark(open(items.length));
          // An expression with nothing picked is in the document; any other kind waits for its value.
          if (canBeLeftUnpicked(kind)) {
            if (argument === undefined) void edits.setArgument(path, listArgument([HOLE]));
            else void edits.insertItem(path, items.length, HOLE);
          } else {
            drafts.set(draftAt, { kind: 'item', place: { step: step.id, path } });
          }
        }}
      />
    </>
  );
}

/** Draws the entries of a `pairs` parameter: the key of each beside its value. */
function PairsDrawing({ parameter, argument, path, around }: ListProps): React.JSX.Element {
  const fresh = useContext(FreshContext);
  const edits = useStepEdits();
  const step = useStepAt();
  const drafts = useDrafts();
  const { kind } = parameter;
  if (kind.kind !== 'pairs') return <></>;
  const entries = argument?.kind === 'pairs' ? argument.entries : [];
  const draftAt = draftKey('entry', step.id, pathKey(path));
  const draft = drafts.get(draftAt);
  // Keyword arguments are named; a dictionary maps one thing to another.
  const between = pairsJoiner(parameter);
  const described = describe(parameter);
  // An entry of pairs is taken away; nothing is added after it, a faint word adds one at the end.
  const entryActions = actionsOf('entry');
  /** Puts a new entry in the document once both sides are known: an expression may wait as a hole. */
  const send = (key: ValueRequest | null, value: ValueRequest | null): void => {
    const held = value ?? (canBeLeftUnpicked(kind.value) ? HOLE : null);
    if (key === null || held === null) {
      drafts.set(draftAt, { kind: 'entry', key, value, place: { step: step.id, path } });
      return;
    }
    drafts.clear(draftAt);
    if (argument === undefined) {
      void edits.setArgument(path, { kind: 'pairs', entries: [{ key, value: held }] });
    } else {
      void edits.insertEntry(path, entries.length, key, held);
    }
  };
  return (
    <>
      {entries.map((entry, at) => {
        const keyPath = under(path, keySegment(at));
        const valuePath = under(path, valueSegment(at));
        return (
          <Piece
            key={at}
            remove={
              entryActions.remove
                ? {
                    label: 'Remove this entry',
                    onPress: () => {
                      void edits.removeEntry(keyPath);
                    },
                  }
                : null
            }
            add={noAddition(entryActions)}
          >
            <OneDrawing
              kind={kind.key}
              parameter={parameter}
              argument={entry.key}
              path={keyPath}
              describedAs={`${described}, the name`}
              around={around}
              names
              openId={freshKey(step.id, keyPath)}
              onSet={(request) => {
                void edits.setArgument(keyPath, request);
              }}
            />
            <Word>{between}</Word>
            <OneDrawing
              kind={kind.value}
              parameter={parameter}
              argument={entry.value}
              path={valuePath}
              describedAs={`${described}, what it holds`}
              around={around}
              onSet={(request) => {
                void edits.setArgument(valuePath, request);
              }}
            />
          </Piece>
        );
      })}
      {draft?.kind === 'entry' && (
        // An entry with a side still to be picked is not in the document: it waits here.
        <Piece
          remove={
            entryActions.remove
              ? {
                  label: 'Remove this entry',
                  onPress: () => {
                    drafts.clear(draftAt);
                  },
                }
              : null
          }
          add={noAddition(entryActions)}
        >
          <OneDrawing
            kind={kind.key}
            parameter={parameter}
            argument={undefined}
            path={null}
            describedAs={`${described}, the name`}
            around={around}
            names
            openId={freshKey(step.id, under(path, keySegment(entries.length)))}
            onSet={(request) => {
              send(request, draft.value);
            }}
          />
          <Word>{between}</Word>
          <OneDrawing
            kind={kind.value}
            parameter={parameter}
            argument={undefined}
            path={null}
            describedAs={`${described}, what it holds`}
            around={around}
            onSet={(request) => {
              send(draft.key, request);
            }}
          />
        </Piece>
      )}
      <Block
        label="+"
        tone="ink"
        look="plain"
        waits
        describedAs={`Add one to ${parameter.shown}`}
        onPress={() => {
          fresh.mark(freshKey(step.id, under(path, keySegment(entries.length))));
          drafts.set(draftAt, {
            kind: 'entry',
            key: null,
            value: null,
            place: { step: step.id, path },
          });
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
  readonly value: string;
  readonly onChange: (request: ValueRequest) => void;
}): React.JSX.Element {
  const { kind } = parameter;
  const label = `${parameter.shown} for ${item}`;
  const control = controlOf(parameter);
  const chosen = control.shape === 'perItem' && control.each === 'choose';
  if (chosen && (kind.kind === 'flag' || kind.kind === 'choice')) {
    // A setting with a few fixed answers is chosen from a list inside the list.
    const options =
      kind.kind === 'flag'
        ? [
            { word: 'true', shown: 'true' },
            { word: 'false', shown: 'false' },
          ]
        : kind.words;
    return (
      <select
        aria-label={label}
        className="dg-given"
        value={value}
        onChange={(event) => {
          const word = event.currentTarget.value;
          onChange(kind.kind === 'flag' ? flagArgument(word === 'true') : wordArgument(kind, word));
        }}
      >
        {value === '' && <option value="">{'—'}</option>}
        {options.map((choice) => (
          <option key={choice.word} value={choice.word}>
            {choice.shown}
          </option>
        ))}
      </select>
    );
  }
  return (
    <span className="font-sans text-xs text-wrong">
      {control.shape === 'perItem' && control.each === 'none'
        ? control.why
        : `${parameter.shown} is not drawn here`}
    </span>
  );
}

/** Returns the word a flag or choice argument is, as the controls hold it, or the empty text. */
function heldWord(argument: ArgumentView | undefined): string {
  if (argument?.kind === 'flag') return argument.value === null ? '' : String(argument.value);
  return argument?.kind === 'choice' ? argument.word : '';
}

/**
 * Draws a parameter that holds one value for each item of another, such as the way each field of a
 * sort is ordered. Its list sets one value for every item, or a value for each.
 */
function EachDrawing({ parameter, argument, path, around }: ListProps): React.JSX.Element {
  const edits = useStepEdits();
  const { kind, shape } = parameter;
  const items = shape.shape === 'perItem' ? around.items(shape.of) : [];
  // The path of the call the parameter is of: the way to the parameter, without it.
  const callPath = path.slice(0, -1);
  const fallback = defaultWordOf(parameter);
  const valueFor = (index: number): string => {
    const own = argument?.kind === 'many' ? argument.items[index] : argument;
    const held = heldWord(own);
    return held === '' ? fallback : held;
  };
  const shownWord = (word: string): { label: string; quoted: boolean } => {
    if (word === '') return { label: 'default', quoted: false };
    const shown =
      kind.kind === 'choice' ? kind.words.find((choice) => choice.word === word)?.shown : undefined;
    return { label: shown ?? word, quoted: false };
  };
  const entries: Entry[] = items.map((item, index) => {
    const word = shownWord(valueFor(index));
    return {
      id: item,
      wrong: false,
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
              void edits.setArgument(path, flagArgument(flag));
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
                void edits.setArgument(path, wordArgument(kind, choice.word));
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
            .map((item, index) => ({ item, index }))
            .filter(({ item }) => matches(typed, item))
            .map(({ item, index }) => {
              const word = valueFor(index);
              return offer(
                item,
                'column',
                shownWord(word).label,
                () => {
                  // A flag turns over when its row is pressed; anything else is set in the control.
                  if (kind.kind === 'flag' && word !== '') {
                    void edits.setForItem(
                      callPath,
                      parameter.name,
                      index,
                      flagArgument(word !== 'true'),
                    );
                  }
                },
                {
                  id: `each:${item}`,
                  act: true,
                  extra: (
                    <EachControl
                      parameter={parameter}
                      item={item}
                      value={word}
                      onChange={(request) => {
                        void edits.setForItem(callPath, parameter.name, index, request);
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
  /** What the document holds for the parameter, or `undefined` where it is not given. */
  readonly argument: ArgumentView | undefined;
  /** The way to the argument in the step. */
  readonly path: Path;
  readonly around: Surroundings;
}

/**
 * Draws a parameter by its kind and shape: the kind's own drawing for one,
 * a list of them for many, or a value for each item of the parameter it
 * follows.
 */
export function ParameterDrawing({
  parameter,
  argument,
  path,
  around,
}: ParameterDrawingProps): React.JSX.Element {
  const edits = useStepEdits();
  const step = useStepAt();
  const { kind } = parameter;
  const notes = [`${parameter.shown} · ${parameter.label}`];
  const control = controlOf(parameter);
  let drawing: React.JSX.Element;
  switch (control.shape) {
    case 'perItem':
      drawing = (
        <EachDrawing parameter={parameter} argument={argument} path={path} around={around} />
      );
      break;
    case 'pairs':
      drawing = (
        <PairsDrawing parameter={parameter} argument={argument} path={path} around={around} />
      );
      break;
    case 'columns':
    case 'many':
      drawing = (
        <ManyDrawing parameter={parameter} argument={argument} path={path} around={around} />
      );
      break;
    case 'one':
      drawing = (
        <OneDrawing
          kind={kind}
          parameter={parameter}
          argument={argument}
          path={path}
          describedAs={describe(parameter)}
          around={around}
          openId={freshKey(step.id, path)}
          onSet={(request) => {
            void edits.setArgument(path, request);
          }}
        />
      );
      break;
    default:
      return control satisfies never;
  }
  return (
    <Hint lines={notes} code={false}>
      {drawing}
    </Hint>
  );
}

/**
 * Returns what a parameter that is added is first set to, and why it cannot
 * be: the engine says which word a flag or a choice starts with, and where
 * Polars refuses every word but the default, the reason. Any other kind is added by asking
 * the engine to start it. While the engine has not said, a flag or a choice waits for it.
 */
function reasonToWait(
  parameter: ParameterView,
  engine: Asked<ReadonlyMap<string, PreparedOption>>,
): string | null {
  if (!startsWithEngineWord(parameter.kind)) return null;
  if (engine.state === 'failed') return engine.reason;
  if (engine.state !== 'ready') return 'waiting for the engine';
  const option = engine.answer.get(parameter.name);
  if (option === undefined) return null;
  return option.starter === null ? option.stuck : null;
}

/**
 * Lists the parameters that can be added, with those that are given ticked: picking one adds it or
 * takes it away. A flag or a choice starts with the word the engine says it may have beside what is
 * already set, and cannot be added where Polars refuses every other word.
 */
export function optionOffers(
  parameters: readonly ParameterView[],
  given: ReadonlySet<string>,
  typed: string,
  onToggle: (parameter: ParameterView, given: boolean) => void,
  engine: Asked<ReadonlyMap<string, PreparedOption>>,
): OfferGroup {
  return counted(
    'Options',
    parameters
      .filter((parameter) => matches(typed, parameter.shown, parameter.label, parameter.name))
      .map((parameter) => {
        const set = given.has(parameter.name);
        return offer(
          parameter.shown,
          'ink',
          parameter.label,
          () => {
            onToggle(parameter, set);
          },
          {
            id: `option:${parameter.name}`,
            ticked: set,
            reason: set ? null : reasonToWait(parameter, engine),
          },
        );
      }),
  );
}

/**
 * Returns what toggles an option of the call at `callPath`: a given one is taken away, and one that
 * is not is added as the engine starts it. An option that starts with nothing a document holds, such
 * as text still to be typed, waits as a draft until it has its value.
 */
export function useOptionToggle(
  callPath: Path,
): (parameter: ParameterView, given: boolean) => void {
  const edits = useStepEdits();
  const step = useStepAt();
  const drafts = useDrafts();
  const fresh = useContext(FreshContext);
  return (parameter, given) => {
    const path = under(callPath, parameterSegment(parameter.name));
    const draftAt = draftKey('option', step.id, pathKey(path));
    if (given || drafts.get(draftAt) !== undefined) {
      drafts.clear(draftAt);
      if (given) void edits.clearArgument(path);
      return;
    }
    fresh.mark(freshKey(step.id, path));
    void edits.addOption(callPath, parameter.name).then((sent) => {
      if (sent.status === 'refused' && handlingOf(sent.problem) === 'askForValue') {
        drafts.set(draftAt, { kind: 'option', place: { step: step.id, path } });
      }
    });
  };
}

/** What the parameters that are set show and report. */
export interface OptionRowsProps {
  readonly entry: EntryView | null;
  /** The call the parameters are of. */
  readonly call: CallView;
  /** The way to the call in the step. */
  readonly callPath: Path;
  /** The parameters that may be added: the entry's, less those the owner draws itself. */
  readonly parameters: readonly ParameterView[];
  readonly around: Surroundings;
  /** Whether each parameter stands on a line of its own beneath the owner, or all follow it on its line. */
  readonly layout: 'inline' | 'lines';
}

/** Draws every parameter of an owner that is set, and any that is waiting for its value. */
export function OptionRows({
  entry,
  call,
  callPath,
  parameters,
  around,
  layout,
}: OptionRowsProps): React.JSX.Element | null {
  // Whether a parameter is at what Polars holds when it is not given is the engine's to say.
  const atDefault = usePreparedStep()?.atDefault;
  const drafts = useDrafts();
  const step = useStepAt();
  const edits = useStepEdits();
  if (entry === null) return null;
  const given = entry.parameters.flatMap((parameter) => {
    const held = call.arguments.find((candidate) => candidate.parameter === parameter.name);
    return held === undefined || !parameters.some((offered) => offered.name === parameter.name)
      ? []
      : [{ parameter, argument: held.value }];
  });
  const waiting = parameters.filter(
    (parameter) =>
      !given.some((held) => held.parameter.name === parameter.name) &&
      drafts.get(
        draftKey('option', step.id, pathKey(under(callPath, parameterSegment(parameter.name)))),
      ) !== undefined,
  );
  const row = (
    parameter: ParameterView,
    argument: ArgumentView | undefined,
    path: Path,
  ): React.JSX.Element => (
    <Row key={parameter.name} layout={layout}>
      <Word>{parameter.shown}</Word>
      {argument === undefined ? (
        // An option that has no value yet is not in the document: it waits here until it has one.
        <OneDrawing
          kind={parameter.kind}
          parameter={parameter}
          argument={undefined}
          path={null}
          describedAs={describe(parameter)}
          around={around}
          openId={freshKey(step.id, path)}
          onSet={(request) => {
            drafts.clear(draftKey('option', step.id, pathKey(path)));
            void edits.setArgument(path, request);
          }}
        />
      ) : (
        <ParameterDrawing parameter={parameter} argument={argument} path={path} around={around} />
      )}
      {argument !== undefined && atDefault?.has(pathKey(path)) === true && (
        <span className="font-sans text-xs text-muted">same as the default</span>
      )}
    </Row>
  );
  const rows = [
    ...given.map(({ parameter, argument }) =>
      row(parameter, argument, under(callPath, parameterSegment(parameter.name))),
    ),
    ...waiting.map((parameter) =>
      row(parameter, undefined, under(callPath, parameterSegment(parameter.name))),
    ),
  ];
  if (rows.length === 0) return null;
  return layout === 'lines' ? <Lines>{rows}</Lines> : <>{rows}</>;
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
