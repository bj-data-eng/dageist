/**
 * @file Reads the catalogue of operations the host sends. See `CatalogueView`
 * in the contract.
 */

import type {
  CatalogueView,
  ChoiceView,
  NoneView,
  ComparisonView,
  DefaultView,
  EntryView,
  KindView,
  PairView,
  ParameterView,
  PositionView,
  SettingView,
  ShapeView,
  WordsView,
} from './contract';
import { APPLIES, NESTED_POSITIONS, OUTER_POSITIONS, VALUE_TYPES } from './contract';
import type { Decode } from './decode';
import {
  ROOT,
  readBoolean,
  readField,
  readInteger,
  readList,
  readNullable,
  readNumber,
  readRecord,
  readString,
  readTagged,
  readWord,
} from './decode';

const readNone: Decode<NoneView> = readRecord(['shown', 'hint'], (fields, path) => ({
  shown: readField(fields, path, 'shown', readString),
  hint: readField(fields, path, 'hint', readString),
}));

const readChoice: Decode<ChoiceView> = readRecord(['word', 'shown'], (fields, path) => ({
  word: readField(fields, path, 'word', readString),
  shown: readField(fields, path, 'shown', readString),
}));

const readKind: Decode<KindView> = readTagged<KindView>('kind', {
  expression: { fields: [], read: () => ({ kind: 'expression' }) },
  column: { fields: [], read: () => ({ kind: 'column' }) },
  value: {
    fields: ['type'],
    read: (fields, path) => ({
      kind: 'value',
      type: readField(fields, path, 'type', readWord(VALUE_TYPES)),
    }),
  },
  flag: { fields: [], read: () => ({ kind: 'flag' }) },
  choice: {
    fields: ['words'],
    read: (fields, path) => ({
      kind: 'choice',
      words: readField(fields, path, 'words', readList(readChoice)),
    }),
  },
  dtype: { fields: [], read: () => ({ kind: 'dtype' }) },
  duration: { fields: [], read: () => ({ kind: 'duration' }) },
  pattern: {
    fields: ['plainTextFlag'],
    read: (fields, path) => ({
      kind: 'pattern',
      plainTextFlag: readField(fields, path, 'plainTextFlag', readNullable(readString)),
    }),
  },
  timeZone: {
    fields: ['none'],
    read: (fields, path) => ({
      kind: 'timeZone',
      none: readField(fields, path, 'none', readNullable(readNone)),
    }),
  },
  table: { fields: [], read: () => ({ kind: 'table' }) },
  path: { fields: [], read: () => ({ kind: 'path' }) },
  pairs: {
    fields: ['key', 'value'],
    read: (fields, path) => ({
      kind: 'pairs',
      // A pair's two sides are kinds in their own right, so a kind reads itself.
      key: readField(fields, path, 'key', (value, at) => readKind(value, at)),
      value: readField(fields, path, 'value', (value, at) => readKind(value, at)),
    }),
  },
});

const readShape: Decode<ShapeView> = readTagged<ShapeView>('shape', {
  one: { fields: [], read: () => ({ shape: 'one' }) },
  many: { fields: [], read: () => ({ shape: 'many' }) },
  perItem: {
    fields: ['of'],
    read: (fields, path) => ({
      shape: 'perItem',
      of: readField(fields, path, 'of', readString),
    }),
  },
});

const readDefault: Decode<DefaultView> = readTagged<DefaultView>('kind', {
  required: { fields: [], read: () => ({ kind: 'required' }) },
  none: { fields: [], read: () => ({ kind: 'none' }) },
  flag: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'flag',
      value: readField(fields, path, 'value', readBoolean),
    }),
  },
  integer: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'integer',
      value: readField(fields, path, 'value', readInteger),
    }),
  },
  float: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'float',
      value: readField(fields, path, 'value', readNumber),
    }),
  },
  text: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'text',
      value: readField(fields, path, 'value', readString),
    }),
  },
  empty: { fields: [], read: () => ({ kind: 'empty' }) },
  other: { fields: [], read: () => ({ kind: 'other' }) },
});

const readSetting: Decode<SettingView> = readTagged<SettingView>('kind', {
  unset: { fields: [], read: () => ({ kind: 'unset' }) },
  flag: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'flag',
      value: readField(fields, path, 'value', readBoolean),
    }),
  },
  integer: {
    fields: ['value', 'shown'],
    read: (fields, path) => ({
      kind: 'integer',
      value: readField(fields, path, 'value', readInteger),
      shown: readField(fields, path, 'shown', readString),
    }),
  },
  float: {
    fields: ['value', 'shown'],
    read: (fields, path) => ({
      kind: 'float',
      value: readField(fields, path, 'value', readNumber),
      shown: readField(fields, path, 'shown', readString),
    }),
  },
  text: {
    fields: ['value', 'shown'],
    read: (fields, path) => ({
      kind: 'text',
      value: readField(fields, path, 'value', readString),
      shown: readField(fields, path, 'shown', readString),
    }),
  },
  word: {
    fields: ['value'],
    read: (fields, path) => ({
      kind: 'word',
      value: readField(fields, path, 'value', readString),
    }),
  },
  field: {
    fields: ['column'],
    read: (fields, path) => ({
      kind: 'field',
      column: readField(fields, path, 'column', readString),
    }),
  },
  element: {
    fields: ['method'],
    read: (fields, path) => ({
      kind: 'element',
      method: readField(fields, path, 'method', readString),
    }),
  },
  comparison: {
    fields: ['entry', 'field', 'other'],
    read: (fields, path) => ({
      kind: 'comparison',
      entry: readField(fields, path, 'entry', readString),
      // What is compared are settings in their own right, so a setting reads itself.
      field: readField(fields, path, 'field', (value, at) => readSetting(value, at)),
      other: readField(fields, path, 'other', (value, at) => readSetting(value, at)),
    }),
  },
  list: {
    fields: ['items'],
    read: (fields, path) => ({
      kind: 'list',
      // A list's items are settings in their own right, so a setting reads itself.
      items: readField(
        fields,
        path,
        'items',
        readList((value, at) => readSetting(value, at)),
      ),
    }),
  },
  pairs: {
    fields: ['entries'],
    read: (fields, path) => ({
      kind: 'pairs',
      entries: readField(fields, path, 'entries', readList(readPair)),
    }),
  },
});

/** Reads one entry of pairs, whose two sides are settings in their own right. */
function readPair(value: unknown, path: string): PairView {
  return readRecord(['key', 'value'], (fields, at) => ({
    key: readField(fields, at, 'key', readSetting),
    value: readField(fields, at, 'value', readSetting),
  }))(value, path);
}

const readPosition: Decode<PositionView> = readTagged<PositionView>('kind', {
  outer: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'outer',
      name: readField(fields, path, 'name', readWord(OUTER_POSITIONS)),
    }),
  },
  nested: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'nested',
      name: readField(fields, path, 'name', readWord(NESTED_POSITIONS)),
    }),
  },
});

const readParameter: Decode<ParameterView> = readRecord(
  ['name', 'shown', 'label', 'kind', 'shape', 'default', 'defaultSetting', 'position', 'offered'],
  (fields, path) => ({
    name: readField(fields, path, 'name', readString),
    shown: readField(fields, path, 'shown', readString),
    label: readField(fields, path, 'label', readString),
    kind: readField(fields, path, 'kind', readKind),
    shape: readField(fields, path, 'shape', readShape),
    default: readField(fields, path, 'default', readDefault),
    defaultSetting: readField(fields, path, 'defaultSetting', readNullable(readSetting)),
    position: readField(fields, path, 'position', readNullable(readPosition)),
    offered: readField(fields, path, 'offered', readBoolean),
  }),
);

const readEntry: Decode<EntryView> = readRecord(
  ['id', 'applies', 'family', 'name', 'hint', 'head', 'lazy', 'parameters'],
  (fields, path) => ({
    id: readField(fields, path, 'id', readString),
    applies: readField(fields, path, 'applies', readWord(APPLIES)),
    family: readField(fields, path, 'family', readString),
    name: readField(fields, path, 'name', readString),
    hint: readField(fields, path, 'hint', readString),
    head: readField(fields, path, 'head', readNullable(readString)),
    lazy: readField(fields, path, 'lazy', readBoolean),
    parameters: readField(fields, path, 'parameters', readList(readParameter)),
  }),
);

const readWords: Decode<WordsView> = readRecord(['key', 'title'], (fields, path) => ({
  key: readField(fields, path, 'key', readString),
  title: readField(fields, path, 'title', readString),
}));

const readComparison: Decode<ComparisonView> = readRecord(['entry', 'sign'], (fields, path) => ({
  entry: readField(fields, path, 'entry', readString),
  sign: readField(fields, path, 'sign', readString),
}));

const readCatalogue: Decode<CatalogueView> = readRecord(
  ['families', 'listedDtypes', 'comparisons', 'entries'],
  (fields, path) => ({
    families: readField(fields, path, 'families', readList(readWords)),
    listedDtypes: readField(fields, path, 'listedDtypes', readList(readString)),
    comparisons: readField(fields, path, 'comparisons', readList(readComparison)),
    entries: readField(fields, path, 'entries', readList(readEntry)),
  }),
);

/**
 * Reads what the host's `operation_catalogue` command sent.
 *
 * @throws {DecodeError} When it is not a catalogue, saying where and why.
 */
export function decodeCatalogue(value: unknown): CatalogueView {
  return readCatalogue(value, ROOT);
}
