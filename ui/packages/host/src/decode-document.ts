/**
 * @file Reads the document the host holds for the node, and the answer to an edit of it. See
 * `DocumentStateView` and `EditAnswerView` in the contract. Every closed list of words is read
 * against the contract's own list, which a test compares with the host's.
 */

import type {
  ArgumentView,
  CallView,
  DocumentStateView,
  DocumentStepView,
  DocumentView,
  EditAnswerView,
  GivenView,
  PairsEntryView,
  ProblemView,
  StartView,
  ThingView,
} from './contract';
import { THING_KINDS, VALUE_KINDS, WANTED } from './contract';
import type { Decode } from './decode';
import {
  ROOT,
  readBoolean,
  readCount,
  readField,
  readList,
  readNullable,
  readRecord,
  readString,
  readTagged,
  readWord,
} from './decode';
import { readPlace, readRun } from './decode-run';

/** The reader of arguments by their kind, built the first time one is read. */
let argumentReader: Decode<ArgumentView> | null = null;

/** Reads an argument. An argument holds arguments, so this is a function and not a table. */
function readArgument(value: unknown, path: string): ArgumentView {
  argumentReader ??= readArgumentTagged();
  return argumentReader(value, path);
}

const readCall: Decode<CallView> = readRecord(['entry', 'arguments'], (fields, path) => ({
  entry: readField(fields, path, 'entry', readString),
  arguments: readField(fields, path, 'arguments', readList(readGiven)),
}));

function readGiven(value: unknown, path: string): GivenView {
  return readRecord(['parameter', 'value'], (fields, at) => ({
    parameter: readField(fields, at, 'parameter', readString),
    value: readField(fields, at, 'value', readArgument),
  }))(value, path);
}

const readStart: Decode<StartView> = readTagged<StartView>('kind', {
  call: {
    fields: ['call'],
    read: (fields, path) => ({ kind: 'call', call: readField(fields, path, 'call', readCall) }),
  },
  input: {
    fields: ['name'],
    read: (fields, path) => ({ kind: 'input', name: readField(fields, path, 'name', readString) }),
  },
  hole: { fields: [], read: () => ({ kind: 'hole' }) },
});

const readPairsEntry: Decode<PairsEntryView> = readRecord(['key', 'value'], (fields, path) => ({
  key: readField(fields, path, 'key', readArgument),
  value: readField(fields, path, 'value', readArgument),
}));

/** Returns the reader of an argument by its kind. */
function readArgumentTagged(): Decode<ArgumentView> {
  return readTagged<ArgumentView>('kind', {
    expression: {
      fields: ['start', 'links'],
      read: (fields, path) => ({
        kind: 'expression',
        start: readField(fields, path, 'start', readStart),
        links: readField(fields, path, 'links', readList(readCall)),
      }),
    },
    columns: {
      fields: ['names'],
      read: (fields, path) => ({
        kind: 'columns',
        names: readField(fields, path, 'names', readList(readString)),
      }),
    },
    integer: {
      fields: ['shown'],
      read: (fields, path) => ({
        kind: 'integer',
        shown: readField(fields, path, 'shown', readString),
      }),
    },
    float: {
      fields: ['shown'],
      read: (fields, path) => ({
        kind: 'float',
        shown: readField(fields, path, 'shown', readString),
      }),
    },
    string: {
      fields: ['value'],
      read: (fields, path) => ({
        kind: 'string',
        value: readField(fields, path, 'value', readString),
      }),
    },
    flag: {
      fields: ['value'],
      read: (fields, path) => ({
        kind: 'flag',
        value: readField(fields, path, 'value', readNullable(readBoolean)),
      }),
    },
    choice: {
      fields: ['word'],
      read: (fields, path) => ({
        kind: 'choice',
        word: readField(fields, path, 'word', readString),
      }),
    },
    dtype: {
      fields: ['name'],
      read: (fields, path) => ({
        kind: 'dtype',
        name: readField(fields, path, 'name', readString),
      }),
    },
    duration: {
      fields: ['text'],
      read: (fields, path) => ({
        kind: 'duration',
        text: readField(fields, path, 'text', readString),
      }),
    },
    pattern: {
      fields: ['text'],
      read: (fields, path) => ({
        kind: 'pattern',
        text: readField(fields, path, 'text', readString),
      }),
    },
    timeZone: {
      fields: ['zone'],
      read: (fields, path) => ({
        kind: 'timeZone',
        zone: readField(fields, path, 'zone', readNullable(readString)),
      }),
    },
    table: {
      fields: ['name'],
      read: (fields, path) => ({
        kind: 'table',
        name: readField(fields, path, 'name', readString),
      }),
    },
    path: {
      fields: ['text'],
      read: (fields, path) => ({ kind: 'path', text: readField(fields, path, 'text', readString) }),
    },
    pairs: {
      fields: ['entries'],
      read: (fields, path) => ({
        kind: 'pairs',
        entries: readField(fields, path, 'entries', readList(readPairsEntry)),
      }),
    },
    many: {
      fields: ['items'],
      read: (fields, path) => ({
        kind: 'many',
        items: readField(fields, path, 'items', readList(readArgument)),
      }),
    },
  });
}

const readDocumentStep: Decode<DocumentStepView> = readRecord(['id', 'call'], (fields, path) => ({
  id: readField(fields, path, 'id', readString),
  call: readField(fields, path, 'call', readCall),
}));

export const readDocument: Decode<DocumentView> = readRecord(['steps'], (fields, path) => ({
  steps: readField(fields, path, 'steps', readList(readDocumentStep)),
}));

const readThing: Decode<ThingView> = (value, path) =>
  readTagged<ThingView>(
    'kind',
    Object.fromEntries(
      THING_KINDS.map((kind) => [
        kind,
        kind === 'value'
          ? {
              fields: ['of'],
              read: (fields: Readonly<Record<string, unknown>>, at: string): ThingView => ({
                kind: 'value',
                of: readField(fields, at, 'of', readWord(VALUE_KINDS)),
              }),
            }
          : {
              fields: [],
              read: (): ThingView => ({ kind }),
            },
      ]),
    ),
  )(value, path);

const readProblem: Decode<ProblemView> = readTagged<ProblemView>('kind', {
  noStep: {
    fields: ['steps'],
    read: (fields, path) => ({
      kind: 'noStep',
      steps: readField(fields, path, 'steps', readCount),
    }),
  },
  stepChanged: {
    fields: ['found'],
    read: (fields, path) => ({
      kind: 'stepChanged',
      found: readField(fields, path, 'found', readString),
    }),
  },
  nothing: { fields: [], read: () => ({ kind: 'nothing' }) },
  doesNotFit: {
    fields: ['found'],
    read: (fields, path) => ({
      kind: 'doesNotFit',
      found: readField(fields, path, 'found', readThing),
    }),
  },
  wrongPlace: {
    fields: ['needs', 'found'],
    read: (fields, path) => ({
      kind: 'wrongPlace',
      needs: readField(fields, path, 'needs', readWord(WANTED)),
      found: readField(fields, path, 'found', readThing),
    }),
  },
  notPerItem: { fields: [], read: () => ({ kind: 'notPerItem' }) },
  noDefault: { fields: [], read: () => ({ kind: 'noDefault' }) },
  outOfRange: {
    fields: ['index', 'len'],
    read: (fields, path) => ({
      kind: 'outOfRange',
      index: readField(fields, path, 'index', readCount),
      len: readField(fields, path, 'len', readCount),
    }),
  },
  unknownEntry: {
    fields: ['entry'],
    read: (fields, path) => ({
      kind: 'unknownEntry',
      entry: readField(fields, path, 'entry', readString),
    }),
  },
  noStart: {
    fields: ['entry'],
    read: (fields, path) => ({
      kind: 'noStart',
      entry: readField(fields, path, 'entry', readString),
    }),
  },
  cannotAdd: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'cannotAdd',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
  needsValue: {
    fields: ['parameter'],
    read: (fields, path) => ({
      kind: 'needsValue',
      parameter: readField(fields, path, 'parameter', readString),
    }),
  },
  refused: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'refused',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readState: Decode<DocumentStateView> = readRecord(
  ['revision', 'document', 'run'],
  (fields, path) => ({
    revision: readField(fields, path, 'revision', readCount),
    document: readField(fields, path, 'document', readDocument),
    run: readField(fields, path, 'run', readRun),
  }),
);

const readEditAnswer: Decode<EditAnswerView> = readTagged<EditAnswerView>('status', {
  edited: {
    fields: ['revision', 'document', 'run'],
    read: (fields, path) => ({
      status: 'edited',
      revision: readField(fields, path, 'revision', readCount),
      document: readField(fields, path, 'document', readDocument),
      run: readField(fields, path, 'run', readRun),
    }),
  },
  refused: {
    fields: ['revision', 'place', 'problem', 'message'],
    read: (fields, path) => ({
      status: 'refused',
      revision: readField(fields, path, 'revision', readCount),
      place: readField(fields, path, 'place', readPlace),
      problem: readField(fields, path, 'problem', readProblem),
      message: readField(fields, path, 'message', readString),
    }),
  },
});

/**
 * Reads what the host's `node_document` command sent.
 *
 * @throws {DecodeError} When it is not the document and its report, saying where and why.
 */
export function decodeDocumentState(value: unknown): DocumentStateView {
  return readState(value, ROOT);
}

/**
 * Reads what the host's `edit_document` command sent.
 *
 * @throws {DecodeError} When it is not an answer to an edit, saying where and why.
 */
export function decodeEditAnswer(value: unknown): EditAnswerView {
  return readEditAnswer(value, ROOT);
}
