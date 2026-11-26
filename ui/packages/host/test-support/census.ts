/**
 * @file Test support: reads `census.json`, which the host's tests write from the Rust types: every
 * variant the step editor has to handle. It is not something the host sends, so it is not part of
 * the package's front door. The lists of words are read as words, so that a variant the engine adds
 * reaches the view's test, which holds each against what the editor can draw; a start of a kind the
 * census does not know, or a slot or a kind of parameter of a shape it lacks, is refused here.
 */

import type { TypedKind } from '../src/contract';
import type { Decode } from '../src/decode';
import {
  ROOT,
  readBoolean,
  readField,
  readList,
  readNullable,
  readRecord,
  readString,
  readTagged,
} from '../src/decode';

/** A kind of parameter as the census writes it: its word, a value's type, and each side of pairs. */
export interface CensusKind {
  readonly kind: string;
  readonly type: string | null;
  readonly key: CensusKind | null;
  readonly value: CensusKind | null;
}

/** How a parameter is held: its shape and its kind. */
export interface CensusForm {
  readonly shape: string;
  readonly kind: CensusKind;
}

/** What an expression starts from. */
export type CensusStart =
  | { readonly kind: 'field' }
  | { readonly kind: 'connectedValue' }
  | { readonly kind: 'connectedList' }
  | { readonly kind: 'hole' }
  /** A number, text or flag typed in, by the kind of literal. */
  | { readonly kind: 'typed'; readonly literal: string }
  /** A function that starts a chain, by its entry. */
  | { readonly kind: 'function'; readonly entry: string };

/** A number, text or flag a document holds typed in. */
export interface CensusTypedValue {
  readonly literal: string;
  /** The class of dtype Polars gives it. */
  readonly dtype: string | null;
  /** Whether the host reads typed text as one. */
  readonly readable: boolean;
}

/** How a slot reads a text typed into it: as a literal of a kind, or not at all. */
export interface CensusReading {
  readonly slot: TypedKind;
  readonly literal: string | null;
  readonly dtype: string | null;
}

/** A rule the engine holds that Polars does not, with what it says. */
export interface CensusRule {
  readonly name: string;
  readonly statement: string;
}

/** A reason the engine says it cannot tell whether an expression may end. */
export interface CensusNotKnown {
  readonly name: string;
  readonly because: string;
}

/** A state a step can be reported in. */
export interface CensusState {
  readonly outcome: string;
  /** The cause of the refusal, where the step was refused. */
  readonly cause: string | null;
  readonly rows: string;
  /** What came before it: `nothing`, `refused`, `incomplete`, `failed` or `notRun`. */
  readonly after: string;
}

/** Every variant the step editor has to handle. */
export interface Census {
  readonly starts: readonly CensusStart[];
  readonly typedValues: readonly CensusTypedValue[];
  readonly typedReadings: readonly CensusReading[];
  readonly parameterKinds: readonly string[];
  readonly valueTypes: readonly string[];
  readonly parameterShapes: readonly string[];
  readonly parameterForms: readonly CensusForm[];
  readonly optionKinds: readonly string[];
  readonly defaultKinds: readonly string[];
  readonly settingKinds: readonly string[];
  readonly dtypeClasses: readonly string[];
  readonly valueShapes: readonly string[];
  readonly afterKinds: readonly string[];
  readonly conditionStates: readonly string[];
  readonly resolutions: readonly string[];
  readonly causes: readonly string[];
  readonly refusalSources: readonly string[];
  readonly stages: readonly string[];
  readonly rules: readonly CensusRule[];
  readonly notKnown: readonly CensusNotKnown[];
  readonly outcomes: readonly string[];
  readonly rowsStates: readonly string[];
  readonly changes: readonly string[];
  readonly states: readonly CensusState[];
  readonly editAnswers: readonly string[];
  readonly problemKinds: readonly string[];
  readonly thingKinds: readonly string[];
  readonly valueKinds: readonly string[];
  readonly wanted: readonly string[];
  readonly inputKinds: readonly string[];
  readonly origins: readonly string[];
  readonly cells: readonly string[];
}

const readWords: Decode<readonly string[]> = readList(readString);

/** Reads a kind, which has the fields its word gives it. */
function readKind(value: unknown, path: string): CensusKind {
  const none = { type: null, key: null, value: null };
  return readTagged<CensusKind>('kind', {
    value: {
      fields: ['type'],
      read: (fields, at) => ({
        kind: 'value',
        ...none,
        type: readField(fields, at, 'type', readString),
      }),
    },
    pairs: {
      fields: ['key', 'value'],
      read: (fields, at) => ({
        kind: 'pairs',
        ...none,
        key: readField(fields, at, 'key', readKind),
        value: readField(fields, at, 'value', readKind),
      }),
    },
    ...Object.fromEntries(
      [
        'expression',
        'column',
        'field',
        'flag',
        'choice',
        'dtype',
        'duration',
        'pattern',
        'timeZone',
        'table',
        'path',
      ].map((word) => [word, { fields: [], read: (): CensusKind => ({ kind: word, ...none }) }]),
    ),
  })(value, path);
}

const readForm: Decode<CensusForm> = readRecord(['shape', 'kind'], (fields, path) => ({
  shape: readField(fields, path, 'shape', readString),
  kind: readField(fields, path, 'kind', readKind),
}));

const readStart: Decode<CensusStart> = readTagged<CensusStart>('kind', {
  field: { fields: [], read: () => ({ kind: 'field' }) },
  connectedValue: { fields: [], read: () => ({ kind: 'connectedValue' }) },
  connectedList: { fields: [], read: () => ({ kind: 'connectedList' }) },
  hole: { fields: [], read: () => ({ kind: 'hole' }) },
  typed: {
    fields: ['literal'],
    read: (fields, path) => ({
      kind: 'typed',
      literal: readField(fields, path, 'literal', readString),
    }),
  },
  function: {
    fields: ['entry'],
    read: (fields, path) => ({
      kind: 'function',
      entry: readField(fields, path, 'entry', readString),
    }),
  },
});

const readTypedValue: Decode<CensusTypedValue> = readRecord(
  ['literal', 'dtype', 'readable'],
  (fields, path) => ({
    literal: readField(fields, path, 'literal', readString),
    dtype: readField(fields, path, 'dtype', readNullable(readString)),
    readable: readField(fields, path, 'readable', readBoolean),
  }),
);

const readSlot: Decode<TypedKind> = readTagged<TypedKind>('kind', {
  expression: { fields: [], read: () => ({ kind: 'expression' }) },
  value: {
    fields: ['type'],
    read: (fields, path) => {
      const type = readField(fields, path, 'type', readString);
      switch (type) {
        case 'integer':
        case 'float':
        case 'string':
        case 'date':
        case 'any':
          return { kind: 'value', type };
        default:
          throw new Error(`${path}: ${type} is no type of literal the contract names`);
      }
    },
  },
  duration: { fields: [], read: () => ({ kind: 'duration' }) },
  pattern: { fields: [], read: () => ({ kind: 'pattern' }) },
  timeZone: { fields: [], read: () => ({ kind: 'timeZone' }) },
  path: { fields: [], read: () => ({ kind: 'path' }) },
});

const readReading: Decode<CensusReading> = readRecord(
  ['slot', 'literal', 'dtype'],
  (fields, path) => ({
    slot: readField(fields, path, 'slot', readSlot),
    literal: readField(fields, path, 'literal', readNullable(readString)),
    dtype: readField(fields, path, 'dtype', readNullable(readString)),
  }),
);

const readRule: Decode<CensusRule> = readRecord(['name', 'statement'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  statement: readField(fields, path, 'statement', readString),
}));

const readNotKnown: Decode<CensusNotKnown> = readRecord(['name', 'because'], (fields, path) => ({
  name: readField(fields, path, 'name', readString),
  because: readField(fields, path, 'because', readString),
}));

const readState: Decode<CensusState> = readRecord(
  ['outcome', 'cause', 'rows', 'after'],
  (fields, path) => ({
    outcome: readField(fields, path, 'outcome', readString),
    cause: readField(fields, path, 'cause', readNullable(readString)),
    rows: readField(fields, path, 'rows', readString),
    after: readField(fields, path, 'after', readString),
  }),
);

const readCensus: Decode<Census> = readRecord(
  [
    'starts',
    'typedValues',
    'typedReadings',
    'parameterKinds',
    'valueTypes',
    'parameterShapes',
    'parameterForms',
    'optionKinds',
    'defaultKinds',
    'settingKinds',
    'dtypeClasses',
    'valueShapes',
    'afterKinds',
    'conditionStates',
    'resolutions',
    'causes',
    'refusalSources',
    'stages',
    'rules',
    'notKnown',
    'outcomes',
    'rowsStates',
    'changes',
    'states',
    'editAnswers',
    'problemKinds',
    'thingKinds',
    'valueKinds',
    'wanted',
    'inputKinds',
    'origins',
    'cells',
  ],
  (fields, path) => ({
    starts: readField(fields, path, 'starts', readList(readStart)),
    typedValues: readField(fields, path, 'typedValues', readList(readTypedValue)),
    typedReadings: readField(fields, path, 'typedReadings', readList(readReading)),
    parameterKinds: readField(fields, path, 'parameterKinds', readWords),
    valueTypes: readField(fields, path, 'valueTypes', readWords),
    parameterShapes: readField(fields, path, 'parameterShapes', readWords),
    parameterForms: readField(fields, path, 'parameterForms', readList(readForm)),
    optionKinds: readField(fields, path, 'optionKinds', readWords),
    defaultKinds: readField(fields, path, 'defaultKinds', readWords),
    settingKinds: readField(fields, path, 'settingKinds', readWords),
    dtypeClasses: readField(fields, path, 'dtypeClasses', readWords),
    valueShapes: readField(fields, path, 'valueShapes', readWords),
    afterKinds: readField(fields, path, 'afterKinds', readWords),
    conditionStates: readField(fields, path, 'conditionStates', readWords),
    resolutions: readField(fields, path, 'resolutions', readWords),
    causes: readField(fields, path, 'causes', readWords),
    refusalSources: readField(fields, path, 'refusalSources', readWords),
    stages: readField(fields, path, 'stages', readWords),
    rules: readField(fields, path, 'rules', readList(readRule)),
    notKnown: readField(fields, path, 'notKnown', readList(readNotKnown)),
    outcomes: readField(fields, path, 'outcomes', readWords),
    rowsStates: readField(fields, path, 'rowsStates', readWords),
    changes: readField(fields, path, 'changes', readWords),
    states: readField(fields, path, 'states', readList(readState)),
    editAnswers: readField(fields, path, 'editAnswers', readWords),
    problemKinds: readField(fields, path, 'problemKinds', readWords),
    thingKinds: readField(fields, path, 'thingKinds', readWords),
    valueKinds: readField(fields, path, 'valueKinds', readWords),
    wanted: readField(fields, path, 'wanted', readWords),
    inputKinds: readField(fields, path, 'inputKinds', readWords),
    origins: readField(fields, path, 'origins', readWords),
    cells: readField(fields, path, 'cells', readWords),
  }),
);

/** Reads the census the host wrote, refusing what is not one. */
export function decodeCensus(value: unknown): Census {
  return readCensus(value, ROOT);
}
