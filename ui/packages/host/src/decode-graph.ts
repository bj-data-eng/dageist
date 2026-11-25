/**
 * @file Reads what the host sends of the graph: the graph view the canvas draws, the answer to
 * an edit of it, the layout's revision after a box is placed, and the file a dialog chose. See
 * `GraphView`, `GraphEditAnswerView`, `PlacedView` and `FileView` in the contract.
 */

import type {
  FileView,
  GraphFitView,
  FrameView,
  GraphEditAnswerView,
  GraphParameterView,
  GraphView,
  NodeBoxView,
  PlacedView,
  ReferenceView,
  SlotStateView,
  SlotView,
} from './contract';
import type { Decode } from './decode';
import {
  ROOT,
  readCount,
  readField,
  readInteger,
  readList,
  readNullable,
  readRecord,
  readString,
  readTagged,
  readWord,
} from './decode';
import { readFieldView } from './decode-node';

const readSlot: Decode<SlotView> = readTagged<SlotView>('kind', {
  start: { fields: [], read: () => ({ kind: 'start' }) },
  input: {
    fields: ['name'],
    read: (fields, path) => ({ kind: 'input', name: readField(fields, path, 'name', readString) }),
  },
  written: { fields: [], read: () => ({ kind: 'written' }) },
});

const readReference: Decode<ReferenceView> = readTagged<ReferenceView>('kind', {
  table: {
    fields: ['node'],
    read: (fields, path) => ({ kind: 'table', node: readField(fields, path, 'node', readString) }),
  },
  column: {
    fields: ['node', 'column'],
    read: (fields, path) => ({
      kind: 'column',
      node: readField(fields, path, 'node', readString),
      column: readField(fields, path, 'column', readString),
    }),
  },
  parameter: {
    fields: ['node', 'name'],
    read: (fields, path) => ({
      kind: 'parameter',
      node: readField(fields, path, 'node', readString),
      name: readField(fields, path, 'name', readString),
    }),
  },
});

const readFit: Decode<GraphFitView> = readTagged<GraphFitView>('kind', {
  table: {
    fields: ['node'],
    read: (fields, path) => ({ kind: 'table', node: readField(fields, path, 'node', readString) }),
  },
  parameter: {
    fields: ['node', 'name'],
    read: (fields, path) => ({
      kind: 'parameter',
      node: readField(fields, path, 'node', readString),
      name: readField(fields, path, 'name', readString),
    }),
  },
});

const readSlotState: Decode<SlotStateView> = readRecord(
  ['slot', 'takes', 'from', 'fits'],
  (fields, path) => ({
    slot: readField(fields, path, 'slot', readSlot),
    takes: readField(fields, path, 'takes', readWord(['table', 'value', 'list'] as const)),
    from: readField(fields, path, 'from', readNullable(readReference)),
    fits: readField(fields, path, 'fits', readList(readFit)),
  }),
);

const readFile: Decode<FileView> = readRecord(['path', 'format'], (fields, path) => ({
  path: readField(fields, path, 'path', readString),
  format: readField(fields, path, 'format', readWord(['parquet', 'csv'] as const)),
}));

const readParameter: Decode<GraphParameterView> = readRecord(
  ['name', 'dtype', 'text'],
  (fields, path) => ({
    name: readField(fields, path, 'name', readString),
    dtype: readField(fields, path, 'dtype', readString),
    text: readField(fields, path, 'text', readString),
  }),
);

const readFrame: Decode<FrameView> = readRecord(['x', 'y', 'width', 'height'], (fields, path) => ({
  x: readField(fields, path, 'x', readInteger),
  y: readField(fields, path, 'y', readInteger),
  width: readField(fields, path, 'width', readNullable(readCount)),
  height: readField(fields, path, 'height', readNullable(readCount)),
}));

const readNodeBox: Decode<NodeBoxView> = readRecord(
  [
    'id',
    'name',
    'kind',
    'summary',
    'slots',
    'newInput',
    'columns',
    'parameters',
    'file',
    'problems',
    'frame',
  ],
  (fields, path) => ({
    id: readField(fields, path, 'id', readString),
    name: readField(fields, path, 'name', readString),
    kind: readField(
      fields,
      path,
      'kind',
      readWord(['source', 'transform', 'output', 'parameters'] as const),
    ),
    summary: readField(fields, path, 'summary', readString),
    slots: readField(fields, path, 'slots', readList(readSlotState)),
    newInput: readField(fields, path, 'newInput', readNullable(readList(readFit))),
    columns: readField(fields, path, 'columns', readNullable(readList(readFieldView))),
    parameters: readField(fields, path, 'parameters', readList(readParameter)),
    file: readField(fields, path, 'file', readNullable(readFile)),
    problems: readField(fields, path, 'problems', readList(readString)),
    frame: readField(fields, path, 'frame', readNullable(readFrame)),
  }),
);

const readGraph: Decode<GraphView> = readRecord(
  ['revision', 'layoutRevision', 'nodes', 'parameterDtypes'],
  (fields, path) => ({
    revision: readField(fields, path, 'revision', readCount),
    layoutRevision: readField(fields, path, 'layoutRevision', readCount),
    nodes: readField(fields, path, 'nodes', readList(readNodeBox)),
    parameterDtypes: readField(fields, path, 'parameterDtypes', readList(readString)),
  }),
);

const readGraphEditAnswer: Decode<GraphEditAnswerView> = readTagged<GraphEditAnswerView>('kind', {
  edited: {
    fields: ['graph', 'added'],
    read: (fields, path) => ({
      kind: 'edited',
      graph: readField(fields, path, 'graph', readGraph),
      added: readField(fields, path, 'added', readNullable(readString)),
    }),
  },
  refused: {
    fields: ['revision', 'message'],
    read: (fields, path) => ({
      kind: 'refused',
      revision: readField(fields, path, 'revision', readCount),
      message: readField(fields, path, 'message', readString),
    }),
  },
});

const readPlaced: Decode<PlacedView> = readRecord(['layoutRevision'], (fields, path) => ({
  layoutRevision: readField(fields, path, 'layoutRevision', readCount),
}));

/**
 * Reads what the host's `graph` command sent.
 *
 * @throws {DecodeError} When it is not a graph view, saying where and why.
 */
export function decodeGraph(value: unknown): GraphView {
  return readGraph(value, ROOT);
}

/**
 * Reads what the host's `edit_graph` command sent.
 *
 * @throws {DecodeError} When it is not an answer to an edit of the graph, saying where and why.
 */
export function decodeGraphEditAnswer(value: unknown): GraphEditAnswerView {
  return readGraphEditAnswer(value, ROOT);
}

/**
 * Reads what the host's `place_node` command sent.
 *
 * @throws {DecodeError} When it is not the layout's revision, saying where and why.
 */
export function decodePlaced(value: unknown): PlacedView {
  return readPlaced(value, ROOT);
}

/**
 * Reads what the host's `choose_file` command sent: the file chosen, or `null`.
 *
 * @throws {DecodeError} When it is neither, saying where and why.
 */
export function decodeChosenFile(value: unknown): FileView | null {
  return readNullable(readFile)(value, ROOT);
}

/**
 * Reads what the host's `choose_folder` command sent: the folder chosen, or `null`.
 *
 * @throws {DecodeError} When it is neither, saying where and why.
 */
export function decodeChosenFolder(value: unknown): string | null {
  return readNullable(readString)(value, ROOT);
}
