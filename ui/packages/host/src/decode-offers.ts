/**
 * @file Reads what the host answers of the offers commands: the calls that
 * may stand at a place, which candidates fit a slot, and what the options of
 * an entry may be set to. See `OffersView`, `FitsView` and `OptionsView` in
 * the contract.
 */

import type {
  DefaultArgumentView,
  EntryOfferView,
  FitView,
  FitsView,
  LiteralView,
  OffersView,
  OptionView,
  OptionWordView,
  OptionsView,
  ReadingView,
  ReadingsView,
  StartArgumentView,
  StepStartView,
  StepStartsView,
} from './contract';
import type { Decode } from './decode';
import {
  ROOT,
  readField,
  readInteger,
  readList,
  readNullable,
  readNumber,
  readRecord,
  readString,
  readTagged,
} from './decode';
import { readSettingView } from './decode-catalogue';
import { readOfferPlace, readRefusal, readVerdict } from './decode-run';

const readLiteral: Decode<LiteralView> = readTagged<LiteralView>('kind', {
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
});

const readDefaultArgument: Decode<DefaultArgumentView> = readRecord(
  ['parameter', 'literal'],
  (fields, path) => ({
    parameter: readField(fields, path, 'parameter', readString),
    literal: readField(fields, path, 'literal', readLiteral),
  }),
);

const readEntryOffer: Decode<EntryOfferView> = readRecord(
  ['entry', 'verdict', 'argument'],
  (fields, path) => ({
    entry: readField(fields, path, 'entry', readString),
    verdict: readField(fields, path, 'verdict', readVerdict),
    argument: readField(fields, path, 'argument', readNullable(readDefaultArgument)),
  }),
);

const readOffers: Decode<OffersView> = readRecord(['place', 'entries'], (fields, path) => ({
  place: readField(fields, path, 'place', readOfferPlace),
  entries: readField(fields, path, 'entries', readList(readEntryOffer)),
}));

const readFit: Decode<FitView> = readTagged<FitView>('kind', {
  judged: {
    fields: ['dtype', 'verdict'],
    read: (fields, path) => ({
      kind: 'judged',
      dtype: readField(fields, path, 'dtype', readString),
      verdict: readField(fields, path, 'verdict', readVerdict),
    }),
  },
  unjudged: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'unjudged',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readFits: Decode<FitsView> = readRecord(['fits'], (fields, path) => ({
  fits: readField(fields, path, 'fits', readList(readFit)),
}));

const readReading: Decode<ReadingView> = readTagged<ReadingView>('kind', {
  read: {
    fields: ['literal', 'dtype', 'fit'],
    read: (fields, path) => ({
      kind: 'read',
      literal: readField(fields, path, 'literal', readLiteral),
      dtype: readField(fields, path, 'dtype', readNullable(readString)),
      fit: readField(fields, path, 'fit', readNullable(readFit)),
    }),
  },
  unread: {
    fields: ['reason'],
    read: (fields, path) => ({
      kind: 'unread',
      reason: readField(fields, path, 'reason', readString),
    }),
  },
});

const readReadings: Decode<ReadingsView> = readRecord(['readings'], (fields, path) => ({
  readings: readField(fields, path, 'readings', readList(readReading)),
}));

const readOptionWord: Decode<OptionWordView> = readRecord(['word', 'refusal'], (fields, path) => ({
  word: readField(fields, path, 'word', readString),
  refusal: readField(fields, path, 'refusal', readNullable(readRefusal)),
}));

const readOption: Decode<OptionView> = readRecord(
  ['parameter', 'default', 'starter', 'stuck', 'words'],
  (fields, path) => ({
    parameter: readField(fields, path, 'parameter', readString),
    default: readField(fields, path, 'default', readNullable(readString)),
    starter: readField(fields, path, 'starter', readNullable(readString)),
    stuck: readField(fields, path, 'stuck', readNullable(readRefusal)),
    words: readField(fields, path, 'words', readList(readOptionWord)),
  }),
);

const readOptions: Decode<OptionsView> = readRecord(['options'], (fields, path) => ({
  options: readField(fields, path, 'options', readList(readOption)),
}));

const readStartArgument: Decode<StartArgumentView> = readRecord(
  ['parameter', 'setting'],
  (fields, path) => ({
    parameter: readField(fields, path, 'parameter', readString),
    setting: readField(fields, path, 'setting', readSettingView),
  }),
);

const readStepStart: Decode<StepStartView> = readRecord(['entry', 'arguments'], (fields, path) => ({
  entry: readField(fields, path, 'entry', readString),
  arguments: readField(fields, path, 'arguments', readList(readStartArgument)),
}));

const readStepStarts: Decode<StepStartsView> = readRecord(['starts'], (fields, path) => ({
  starts: readField(fields, path, 'starts', readList(readStepStart)),
}));

/**
 * Reads what the host's `offers_at` command sent.
 *
 * @throws {DecodeError} When it is not the offers at a place, saying where and why.
 */
export function decodeOffers(value: unknown): OffersView {
  return readOffers(value, ROOT);
}

/**
 * Reads what the host's `candidate_fits` command sent.
 *
 * @throws {DecodeError} When it is not the fits of candidates, saying where and why.
 */
export function decodeFits(value: unknown): FitsView {
  return readFits(value, ROOT);
}

/**
 * Reads what the host's `read_typed` command sent.
 *
 * @throws {DecodeError} When it is not the readings of a typed text, saying where and why.
 */
export function decodeReadings(value: unknown): ReadingsView {
  return readReadings(value, ROOT);
}

/**
 * Reads what the host's `option_offers` command sent.
 *
 * @throws {DecodeError} When it is not the options of an entry, saying where and why.
 */
export function decodeOptions(value: unknown): OptionsView {
  return readOptions(value, ROOT);
}

/**
 * Reads what the host's `step_starts` command sent.
 *
 * @throws {DecodeError} When it is not what each step starts with, saying where and why.
 */
export function decodeStepStarts(value: unknown): StepStartsView {
  return readStepStarts(value, ROOT);
}
