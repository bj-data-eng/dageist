import { describe, expect, it } from 'vitest';

import {
  APPLIES,
  CHANGES,
  CONDITION_STATES,
  ERROR_KINDS,
  NESTED_POSITIONS,
  OUTER_POSITIONS,
  PROBLEM_KINDS,
  RULE_NAMES,
  SETTING_KINDS,
  STAGES,
  THING_KINDS,
  VALUE_KINDS,
  VALUE_SHAPES,
  VALUE_TYPES,
  WANTED,
} from './contract';
import { decodeCatalogue } from './decode-catalogue';
import { decodeEditAnswer } from './decode-document';
import { decodeRun } from './decode-run';
import { decodeFailure } from './failure';
import catalogueFixture from '../fixtures/catalogue.json';
import refusedEditFixture from '../fixtures/edit-refused.json';
import refusedFixture from '../fixtures/step-report-refused.json';
import reportFixture from '../fixtures/step-report.json';
import variantsFixture from '../fixtures/step-report-variants.json';
import vocabularyFixture from '../fixtures/vocabulary.json';
import { at, replaced } from '../test-support/tamper';

// The vocabulary is what the host writes of every closed list of words, from the engine's own
// lists of variants. A word the engine adds appears there first, so the decoders failing here is
// how the view learns of it, and not a user's report.
const vocabulary: unknown = vocabularyFixture;
const report: unknown = reportFixture;
const refused: unknown = refusedFixture;
const variants: unknown = variantsFixture;
const catalogue: unknown = catalogueFixture;
const refusedEdit: unknown = refusedEditFixture;

/** The words the host lists under `name`. */
function listed(name: string): readonly string[] {
  const words = at(vocabulary, [name]);
  if (!Array.isArray(words) || !words.every((word: unknown) => typeof word === 'string')) {
    throw new Error(`the list ${name} is not words`);
  }
  return words.map(String);
}

describe('the words the decoders read', () => {
  const cases: ReadonlyArray<{
    readonly list: string;
    readonly words: readonly string[];
    /** Decodes a value with the word in the place that holds it. */
    readonly accepts: (word: string) => unknown;
  }> = [
    {
      list: 'outer',
      words: OUTER_POSITIONS,
      accepts: (word) =>
        decodeRun(
          replaced(report, ['steps', 4, 'walk', 'expressions', 0, 'context', 'outer'], word),
        ),
    },
    {
      list: 'nested',
      words: NESTED_POSITIONS,
      accepts: (word) =>
        decodeRun(
          replaced(report, ['steps', 4, 'walk', 'expressions', 0, 'context', 'nested'], word),
        ),
    },
    {
      list: 'rules',
      words: RULE_NAMES,
      accepts: (word) =>
        decodeRun(
          replaced(refused, ['steps', 1, 'walk', 'refusal', 'refusal', 'source'], {
            by: 'rule',
            rule: word,
          }),
        ),
    },
    {
      list: 'stages',
      words: STAGES,
      accepts: (word) =>
        decodeRun(
          replaced(refused, ['steps', 1, 'walk', 'refusal', 'refusal', 'source'], {
            by: 'polars',
            stage: word,
          }),
        ),
    },
    {
      list: 'conditionStates',
      words: CONDITION_STATES,
      accepts: (word) =>
        decodeRun(
          replaced(variants, ['steps', 0, 'walk', 'expressions', 0, 'calls', 0, 'follows'], {
            kind: 'condition',
            state: word,
          }),
        ),
    },
    {
      list: 'valueShapes',
      words: VALUE_SHAPES,
      accepts: (word) =>
        decodeRun(
          replaced(
            report,
            ['steps', 4, 'walk', 'expressions', 0, 'calls', 1, 'follows', 'shape'],
            word,
          ),
        ),
    },
    {
      list: 'changes',
      words: CHANGES,
      accepts: (word) =>
        decodeRun(replaced(variants, ['steps', 0, 'outcome', 'changes', 0, 'change'], word)),
    },
    {
      list: 'errorKinds',
      words: ERROR_KINDS,
      accepts: (word) => decodeFailure({ kind: word, message: 'the reason' }),
    },
    {
      list: 'applies',
      words: APPLIES,
      accepts: (word) => decodeCatalogue(replaced(catalogue, ['entries', 0, 'applies'], word)),
    },
    {
      list: 'settings',
      words: SETTING_KINDS,
      accepts: (word) =>
        decodeCatalogue(
          replaced(
            catalogue,
            ['entries', shiftIndex(), 'parameters', 0, 'defaultSetting'],
            settingOfKind(word),
          ),
        ),
    },
    {
      list: 'valueKinds',
      words: VALUE_KINDS,
      accepts: (word) =>
        decodeEditAnswer(
          replaced(refusedEdit, ['problem'], {
            kind: 'doesNotFit',
            found: { kind: 'value', of: word },
          }),
        ),
    },
    {
      list: 'wanted',
      words: WANTED,
      accepts: (word) =>
        decodeEditAnswer(
          replaced(refusedEdit, ['problem'], {
            kind: 'wrongPlace',
            needs: word,
            found: { kind: 'step' },
          }),
        ),
    },
    {
      list: 'thingKinds',
      words: THING_KINDS,
      accepts: (word) =>
        decodeEditAnswer(
          replaced(refusedEdit, ['problem'], {
            kind: 'doesNotFit',
            found: word === 'value' ? { kind: word, of: 'flag' } : { kind: word },
          }),
        ),
    },
    {
      list: 'problemKinds',
      words: PROBLEM_KINDS,
      accepts: (word) => decodeEditAnswer(replaced(refusedEdit, ['problem'], problemOfKind(word))),
    },
    {
      list: 'valueTypes',
      words: VALUE_TYPES,
      accepts: (word) =>
        decodeCatalogue(
          replaced(catalogue, ['entries', shiftIndex(), 'parameters', 0, 'kind'], {
            kind: 'value',
            type: word,
          }),
        ),
    },
  ];

  for (const { list, words, accepts } of cases) {
    it(`for ${list} are exactly the words the host lists`, () => {
      expect(new Set(words)).toEqual(new Set(listed(list)));
      expect(words).toHaveLength(listed(list).length);
    });

    it(`for ${list} are each read where they stand`, () => {
      for (const word of listed(list)) {
        expect(() => accepts(word), word).not.toThrow();
      }
    });
  }
});

/** A refusal of the kind `kind`, with what that kind holds. */
function problemOfKind(kind: string): unknown {
  const thing = { kind: 'step' };
  const held: Readonly<Record<string, unknown>> = {
    noStep: { kind: 'noStep', steps: 2 },
    stepChanged: { kind: 'stepChanged', found: 'sort' },
    nothing: { kind: 'nothing' },
    doesNotFit: { kind: 'doesNotFit', found: thing },
    wrongPlace: { kind: 'wrongPlace', needs: 'list', found: thing },
    notPerItem: { kind: 'notPerItem' },
    noDefault: { kind: 'noDefault' },
    outOfRange: { kind: 'outOfRange', index: 5, len: 2 },
    unknownEntry: { kind: 'unknownEntry', entry: 'Expr.nope' },
    noStart: { kind: 'noStart', entry: 'Expr.abs' },
    cannotAdd: { kind: 'cannotAdd', reason: 'because' },
    needsValue: { kind: 'needsValue', parameter: 'name' },
    refused: { kind: 'refused', reason: 'because' },
  };
  const found = held[kind];
  if (found === undefined) throw new Error(`no problem of the kind ${kind}`);
  return found;
}

/** A setting of the kind `kind`, with what that kind holds. */
function settingOfKind(kind: string): unknown {
  const none = { kind: 'unset' };
  const held: Readonly<Record<string, unknown>> = {
    unset: none,
    flag: { kind: 'flag', value: true },
    integer: { kind: 'integer', value: 1, shown: '1' },
    float: { kind: 'float', value: 1.5, shown: '1.5' },
    text: { kind: 'text', value: 'text', shown: 'text' },
    word: { kind: 'word', value: 'word' },
    field: { kind: 'field', column: 'column' },
    element: { kind: 'element', method: 'first' },
    comparison: { kind: 'comparison', entry: 'Expr.eq', field: none, other: none },
    list: { kind: 'list', items: [none] },
    pairs: { kind: 'pairs', entries: [{ key: none, value: none }] },
  };
  const found = held[kind];
  if (found === undefined) throw new Error(`no setting of the kind ${kind}`);
  return found;
}

/** Where `Expr.shift`, an entry with parameters, is among the entries. */
function shiftIndex(): number {
  const decoded = decodeCatalogue(catalogue);
  const found = decoded.entries.findIndex((entry) => entry.id === 'Expr.shift');
  if (found < 0) throw new Error('no entry Expr.shift');
  return found;
}
