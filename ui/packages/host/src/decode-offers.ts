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
import { readOfferPlace, readRefusal, readVerdict } from './decode-run';

const readLiteral: Decode<LiteralView> = readTagged<LiteralView>('kind', {
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

const readOptionWord: Decode<OptionWordView> = readRecord(['word', 'refusal'], (fields, path) => ({
  word: readField(fields, path, 'word', readString),
  refusal: readField(fields, path, 'refusal', readNullable(readRefusal)),
}));

const readOption: Decode<OptionView> = readRecord(
  ['parameter', 'default', 'words'],
  (fields, path) => ({
    parameter: readField(fields, path, 'parameter', readString),
    default: readField(fields, path, 'default', readNullable(readString)),
    words: readField(fields, path, 'words', readList(readOptionWord)),
  }),
);

const readOptions: Decode<OptionsView> = readRecord(['options'], (fields, path) => ({
  options: readField(fields, path, 'options', readList(readOption)),
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
 * Reads what the host's `option_offers` command sent.
 *
 * @throws {DecodeError} When it is not the options of an entry, saying where and why.
 */
export function decodeOptions(value: unknown): OptionsView {
  return readOptions(value, ROOT);
}
