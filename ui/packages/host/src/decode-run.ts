/**
 * @file Reads what running a step document reported, as the host sends it.
 * See `RunView` in the contract.
 */

import type {
  AfterView,
  CauseView,
  ConditionStateView,
  ContextView,
  EndingView,
  ExpressionDtypesView,
  NestedPosition,
  OfferPlaceView,
  OuterPosition,
  OutcomeView,
  PlaceView,
  ReadProblemView,
  RefusalSourceView,
  RefusalView,
  ResolutionView,
  RuleName,
  RunView,
  SegmentView,
  StageView,
  StandingView,
  StepReportView,
  ValueShapeView,
  VerdictView,
  WalkRefusalView,
  WalkView,
  WalkedExpressionView,
} from './contract';
import {
  CONDITION_STATES,
  NESTED_POSITIONS,
  OUTER_POSITIONS,
  RULE_NAMES,
  STAGES,
  VALUE_SHAPES,
} from './contract';
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
import { readFieldView } from './decode-node';

const readSegment: Decode<SegmentView> = readTagged<SegmentView>('kind', {
  parameter: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'parameter',
      name: readField(fields, path, 'name', readString),
    }),
  },
  item: {
    fields: ['index'],
    read: (fields, path) => ({ kind: 'item', index: readField(fields, path, 'index', readCount) }),
  },
  key: {
    fields: ['index'],
    read: (fields, path) => ({ kind: 'key', index: readField(fields, path, 'index', readCount) }),
  },
  value: {
    fields: ['index'],
    read: (fields, path) => ({
      kind: 'value',
      index: readField(fields, path, 'index', readCount),
    }),
  },
  start: { fields: [], read: () => ({ kind: 'start' }) },
  link: {
    fields: ['index'],
    read: (fields, path) => ({ kind: 'link', index: readField(fields, path, 'index', readCount) }),
  },
});

const readPlace: Decode<PlaceView> = readRecord(
  ['stepIndex', 'stepId', 'path'],
  (fields, path) => ({
    stepIndex: readField(fields, path, 'stepIndex', readNullable(readCount)),
    stepId: readField(fields, path, 'stepId', readNullable(readString)),
    path: readField(fields, path, 'path', readList(readSegment)),
  }),
);

const readResolution: Decode<ResolutionView> = readTagged<ResolutionView>('kind', {
  dtype: {
    fields: ['dtype'],
    read: (fields, path) => ({
      kind: 'dtype',
      dtype: readField(fields, path, 'dtype', readString),
    }),
  },
  several: {
    fields: ['dtypes'],
    read: (fields, path) => ({
      kind: 'several',
      dtypes: readField(fields, path, 'dtypes', readList(readString)),
    }),
  },
  unfinished: { fields: [], read: () => ({ kind: 'unfinished' }) },
  noContext: { fields: [], read: () => ({ kind: 'noContext' }) },
  refused: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'refused',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readExpressionDtypes: Decode<ExpressionDtypesView> = readRecord(
  ['place', 'start', 'links'],
  (fields, path) => ({
    place: readField(fields, path, 'place', readPlace),
    start: readField(fields, path, 'start', readResolution),
    links: readField(fields, path, 'links', readList(readResolution)),
  }),
);

const readValueShape: Decode<ValueShapeView> = readWord(VALUE_SHAPES);

const readConditionState: Decode<ConditionStateView> = readWord(CONDITION_STATES);

const readAfter: Decode<AfterView> = readTagged<AfterView>('kind', {
  start: { fields: [], read: () => ({ kind: 'start' }) },
  value: {
    fields: ['dtype', 'shape'],
    read: (fields, path) => ({
      kind: 'value',
      dtype: readField(fields, path, 'dtype', readString),
      shape: readField(fields, path, 'shape', readValueShape),
    }),
  },
  condition: {
    fields: ['state'],
    read: (fields, path) => ({
      kind: 'condition',
      state: readField(fields, path, 'state', readConditionState),
    }),
  },
});

const readOuter: Decode<OuterPosition> = readWord(OUTER_POSITIONS);

const readNested: Decode<NestedPosition> = readWord(NESTED_POSITIONS);

const readContext: Decode<ContextView> = readRecord(['outer', 'nested'], (fields, path) => ({
  outer: readField(fields, path, 'outer', readOuter),
  nested: readField(fields, path, 'nested', readNullable(readNested)),
}));

const readOfferPlace: Decode<OfferPlaceView> = readRecord(['context', 'after'], (fields, path) => ({
  context: readField(fields, path, 'context', readContext),
  after: readField(fields, path, 'after', readAfter),
}));

const readStage: Decode<StageView> = readWord(STAGES);

const readRule: Decode<RuleName> = readWord(RULE_NAMES);

const readRefusalSource: Decode<RefusalSourceView> = readTagged<RefusalSourceView>('by', {
  polars: {
    fields: ['stage'],
    read: (fields, path) => ({
      by: 'polars',
      stage: readField(fields, path, 'stage', readStage),
    }),
  },
  rule: {
    fields: ['rule'],
    read: (fields, path) => ({ by: 'rule', rule: readField(fields, path, 'rule', readRule) }),
  },
  structure: { fields: [], read: () => ({ by: 'structure' }) },
});

const readRefusal: Decode<RefusalView> = readRecord(['source', 'reason'], (fields, path) => ({
  source: readField(fields, path, 'source', readRefusalSource),
  reason: readField(fields, path, 'reason', readString),
}));

const readEnding: Decode<EndingView> = readTagged<EndingView>('kind', {
  allowed: { fields: [], read: () => ({ kind: 'allowed' }) },
  refused: {
    fields: ['refusal'],
    read: (fields, path) => ({
      kind: 'refused',
      refusal: readField(fields, path, 'refusal', readRefusal),
    }),
  },
  notKnown: {
    fields: ['because'],
    read: (fields, path) => ({
      kind: 'notKnown',
      because: readField(fields, path, 'because', readString),
    }),
  },
});

const readVerdict: Decode<VerdictView> = readTagged<VerdictView>('kind', {
  allowed: {
    fields: ['result', 'shape', 'ending'],
    read: (fields, path) => ({
      kind: 'allowed',
      result: readField(fields, path, 'result', readNullable(readString)),
      shape: readField(fields, path, 'shape', readValueShape),
      ending: readField(fields, path, 'ending', readEnding),
    }),
  },
  refused: {
    fields: ['refusal'],
    read: (fields, path) => ({
      kind: 'refused',
      refusal: readField(fields, path, 'refusal', readRefusal),
    }),
  },
});

const readStanding: Decode<StandingView> = readRecord(['follows', 'verdict'], (fields, path) => ({
  follows: readField(fields, path, 'follows', readNullable(readAfter)),
  verdict: readField(fields, path, 'verdict', readNullable(readVerdict)),
}));

const readWalkedExpression: Decode<WalkedExpressionView> = readRecord(
  ['place', 'context', 'calls', 'places', 'ending'],
  (fields, path) => ({
    place: readField(fields, path, 'place', readPlace),
    context: readField(fields, path, 'context', readNullable(readContext)),
    calls: readField(fields, path, 'calls', readList(readStanding)),
    places: readField(fields, path, 'places', readList(readNullable(readOfferPlace))),
    ending: readField(fields, path, 'ending', readNullable(readEnding)),
  }),
);

const readWalkRefusal: Decode<WalkRefusalView> = readRecord(
  ['place', 'refusal', 'atEnd'],
  (fields, path) => ({
    place: readField(fields, path, 'place', readPlace),
    refusal: readField(fields, path, 'refusal', readRefusal),
    atEnd: readField(fields, path, 'atEnd', readBoolean),
  }),
);

const readWalk: Decode<WalkView> = readRecord(
  ['expressions', 'refusal', 'notKnown'],
  (fields, path) => ({
    expressions: readField(fields, path, 'expressions', readList(readWalkedExpression)),
    refusal: readField(fields, path, 'refusal', readNullable(readWalkRefusal)),
    notKnown: readField(fields, path, 'notKnown', readNullable(readString)),
  }),
);

const readCause: Decode<CauseView> = readTagged<CauseView>('kind', {
  unknownTable: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'unknownTable',
      name: readField(fields, path, 'name', readString),
    }),
  },
  unknownInput: {
    fields: ['name'],
    read: (fields, path) => ({
      kind: 'unknownInput',
      name: readField(fields, path, 'name', readString),
    }),
  },
  build: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'build',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
  notAllowed: {
    fields: ['refusal'],
    read: (fields, path) => ({
      kind: 'notAllowed',
      refusal: readField(fields, path, 'refusal', readRefusal),
    }),
  },
  polars: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'polars',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readOutcome: Decode<OutcomeView> = readTagged<OutcomeView>('status', {
  valid: {
    fields: ['columns'],
    read: (fields, path) => ({
      status: 'valid',
      columns: readField(fields, path, 'columns', readList(readFieldView)),
    }),
  },
  refused: {
    fields: ['place', 'cause'],
    read: (fields, path) => ({
      status: 'refused',
      place: readField(fields, path, 'place', readPlace),
      cause: readField(fields, path, 'cause', readCause),
    }),
  },
  notRun: {
    fields: ['blockedBy'],
    read: (fields, path) => ({
      status: 'notRun',
      blockedBy: readField(fields, path, 'blockedBy', readString),
    }),
  },
});

const readStepReport: Decode<StepReportView> = readRecord(
  ['id', 'lazy', 'outcome', 'expressions', 'walk'],
  (fields, path) => ({
    id: readField(fields, path, 'id', readString),
    lazy: readField(fields, path, 'lazy', readBoolean),
    outcome: readField(fields, path, 'outcome', readOutcome),
    expressions: readField(fields, path, 'expressions', readList(readExpressionDtypes)),
    walk: readField(fields, path, 'walk', readWalk),
  }),
);

const readReadProblem: Decode<ReadProblemView> = readTagged<ReadProblemView>('kind', {
  syntax: {
    fields: ['line', 'column', 'reason'],
    read: (fields, path) => ({
      kind: 'syntax',
      line: readField(fields, path, 'line', readCount),
      column: readField(fields, path, 'column', readCount),
      reason: readField(fields, path, 'reason', readString),
    }),
  },
  refused: {
    fields: ['place', 'reason'],
    read: (fields, path) => ({
      kind: 'refused',
      place: readField(fields, path, 'place', readPlace),
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readRun: Decode<RunView> = readTagged<RunView>('status', {
  run: {
    fields: ['source', 'steps'],
    read: (fields, path) => ({
      status: 'run',
      source: readField(fields, path, 'source', readList(readFieldView)),
      steps: readField(fields, path, 'steps', readList(readStepReport)),
    }),
  },
  unreadable: {
    fields: ['problem'],
    read: (fields, path) => ({
      status: 'unreadable',
      problem: readField(fields, path, 'problem', readReadProblem),
    }),
  },
});

/**
 * Reads what the host's `run_document` command sent.
 *
 * @throws {DecodeError} When it is not a report of a run, saying where and why.
 */
export function decodeRun(value: unknown): RunView {
  return readRun(value, ROOT);
}
