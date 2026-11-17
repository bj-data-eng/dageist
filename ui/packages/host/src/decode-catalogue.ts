/**
 * @file Reads the catalogue of operations the host sends. See `CatalogueView`
 * in the contract.
 */

import type {
  CatalogueView,
  ChoiceView,
  DefaultView,
  EntryView,
  KindView,
  ParameterView,
  ShapeView,
  WordsView,
} from './contract';
import { APPLIES, VALUE_TYPES } from './contract';
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

const readChoice: Decode<ChoiceView> = readRecord(['word', 'shown'], (fields, path) => ({
  word: readField(fields, path, 'word', readString),
  shown: readField(fields, path, 'shown', readString),
}));

const readKind: Decode<KindView> = readTagged<KindView>('kind', {
  expression: {
    fields: ['dtypes'],
    read: (fields, path) => ({
      kind: 'expression',
      dtypes: readField(fields, path, 'dtypes', readList(readString)),
    }),
  },
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

const readParameter: Decode<ParameterView> = readRecord(
  ['name', 'shown', 'label', 'kind', 'shape', 'default', 'python'],
  (fields, path) => ({
    name: readField(fields, path, 'name', readString),
    shown: readField(fields, path, 'shown', readString),
    label: readField(fields, path, 'label', readString),
    kind: readField(fields, path, 'kind', readKind),
    shape: readField(fields, path, 'shape', readShape),
    default: readField(fields, path, 'default', readDefault),
    python: readField(fields, path, 'python', readNullable(readString)),
  }),
);

const readEntry: Decode<EntryView> = readRecord(
  ['id', 'applies', 'family', 'name', 'hint', 'head', 'accepts', 'lazy', 'parameters'],
  (fields, path) => ({
    id: readField(fields, path, 'id', readString),
    applies: readField(fields, path, 'applies', readWord(APPLIES)),
    family: readField(fields, path, 'family', readString),
    name: readField(fields, path, 'name', readString),
    hint: readField(fields, path, 'hint', readString),
    head: readField(fields, path, 'head', readNullable(readString)),
    accepts: readField(fields, path, 'accepts', readList(readString)),
    lazy: readField(fields, path, 'lazy', readBoolean),
    parameters: readField(fields, path, 'parameters', readList(readParameter)),
  }),
);

const readWords: Decode<WordsView> = readRecord(['key', 'title'], (fields, path) => ({
  key: readField(fields, path, 'key', readString),
  title: readField(fields, path, 'title', readString),
}));

const readCatalogue: Decode<CatalogueView> = readRecord(
  ['families', 'dtypes', 'entries'],
  (fields, path) => ({
    families: readField(fields, path, 'families', readList(readWords)),
    dtypes: readField(fields, path, 'dtypes', readList(readWords)),
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
