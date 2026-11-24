import { describe, expect, it } from 'vitest';

import type { ArgumentView, CallView, DocumentStateView } from './contract';
import { DecodeError } from './decode';
import { decodeDocumentState, decodeEditAnswer } from './decode-document';
import documentFixture from '../fixtures/node-document.json';
import editedFixture from '../fixtures/edit-edited.json';
import refusedFixture from '../fixtures/edit-refused.json';
import { replaced, withField, without } from '../test-support/tamper';

// The fixtures are what the host's `node_document` command answers for the claim-timing steps, and
// what `edit_document` answers for an edit that was made and one the engine refused, written from
// Rust by its snapshot test. They are read as `unknown`, as the host's answer is, and the decoder
// is what says what they are.
const state: unknown = documentFixture;
const edited: unknown = editedFixture;
const refused: unknown = refusedFixture;

function refusal(decode: (value: unknown) => unknown, value: unknown): DecodeError {
  try {
    decode(value);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

/** The call of the step `id` of a state. */
function callOf(held: DocumentStateView, id: string): CallView {
  const found = held.document.steps.find((step) => step.id === id);
  if (found === undefined) throw new Error(`no step ${id}`);
  return found.call;
}

/** The argument given for `parameter` of `call`. */
function argumentOf(call: CallView, parameter: string): ArgumentView {
  const found = call.arguments.find((given) => given.parameter === parameter);
  if (found === undefined) throw new Error(`no argument ${parameter}`);
  return found.value;
}

describe('the document the host holds for the claim-timing node', () => {
  const held = decodeDocumentState(state);

  it('is at its first revision, with the twelve steps in order and what running them reported', () => {
    expect(held.revision).toBe(1);
    expect(held.document.steps.map((step) => step.id)).toEqual([
      'sort',
      'step_index',
      'resolved_at',
      'received_at',
      'resolved_day',
      'days_open',
      'a_days',
      'join',
      'sla_status',
      'flag',
      'holiday_count',
      'pivot',
    ]);
    expect(held.run.steps.map((step) => step.id)).toEqual(
      held.document.steps.map((step) => step.id),
    );
  });

  it('holds the sort by its two keys, each a field the sort reads', () => {
    expect(callOf(held, 'sort')).toEqual({
      entry: 'LazyFrame.sort',
      arguments: [
        {
          parameter: 'by',
          value: {
            kind: 'many',
            items: [field('dcn'), field('timestamp')],
          },
        },
      ],
    });
  });

  it('holds the join with the table it brings in, its keys and its choices', () => {
    const join = callOf(held, 'join');
    expect(argumentOf(join, 'other')).toEqual({ kind: 'table', name: 'step_sla' });
    expect(argumentOf(join, 'how')).toEqual({ kind: 'choice', word: 'left' });
    expect(argumentOf(join, 'left_on')).toEqual({ kind: 'many', items: [field('step_to')] });
  });

  it('holds a number as the words the engine shows it as, and an expression as its start and links', () => {
    const shift = callOf(held, 'resolved_at');
    const named = argumentOf(shift, 'named_exprs');
    expect(named.kind).toBe('pairs');
    if (named.kind !== 'pairs') return;
    expect(named.entries[0]?.key).toEqual({ kind: 'columns', names: ['resolved_at'] });
    const value = named.entries[0]?.value;
    expect(value?.kind).toBe('expression');
    if (value?.kind !== 'expression') return;
    expect(value.links.map((link) => link.entry)).toEqual(['Expr.shift', 'Expr.over']);
    expect(argumentOf(value.links[0] ?? shift, 'n')).toEqual({
      kind: 'expression',
      start: {
        kind: 'call',
        call: {
          entry: 'polars.lit',
          arguments: [{ parameter: 'value', value: { kind: 'integer', shown: '1' } }],
        },
      },
      links: [],
    });
  });

  it('holds a condition as the call that starts it and the links that finish it', () => {
    const named = argumentOf(callOf(held, 'sla_status'), 'named_exprs');
    const value = named.kind === 'pairs' ? named.entries[0]?.value : undefined;
    expect(value?.kind === 'expression' && value.start).toMatchObject({
      kind: 'call',
      call: { entry: 'polars.when' },
    });
    expect(value?.kind === 'expression' && value.links.map((link) => link.entry)).toEqual([
      'When.then',
      'Then.otherwise',
    ]);
  });
});

function field(name: string): ArgumentView {
  return {
    kind: 'expression',
    start: {
      kind: 'call',
      call: {
        entry: 'polars.col',
        arguments: [{ parameter: 'name', value: { kind: 'columns', names: [name] } }],
      },
    },
    links: [],
  };
}

describe('what the host answers for an edit that was made', () => {
  it('is the status, the new revision, the document and what running it reported', () => {
    const answer = decodeEditAnswer(edited);
    expect(answer.status).toBe('edited');
    if (answer.status !== 'edited') return;
    expect(answer.revision).toBe(2);
    expect(answer.document.steps.map((step) => step.id)).toEqual(['sort', 'head', 'earlier']);
    expect(answer.run.steps).toHaveLength(3);
    // A new `head` is given nothing: Polars takes its five rows.
    expect(answer.document.steps[1]?.call).toEqual({ entry: 'LazyFrame.head', arguments: [] });
  });
});

describe('what the host answers for an edit the engine refused', () => {
  it('is the place, the kind of reason, the words for it and the revision it stays at', () => {
    expect(decodeEditAnswer(refused)).toEqual({
      status: 'refused',
      revision: 1,
      place: { stepIndex: 7, stepId: 'nowhere', path: [] },
      problem: { kind: 'noStep', steps: 2 },
      message: 'step "nowhere": there is no such step: the document has 2',
    });
  });

  it('reads a reason that names what was there as a kind of thing', () => {
    const answer = decodeEditAnswer(
      replaced(refused, ['problem'], {
        kind: 'wrongPlace',
        needs: 'list',
        found: { kind: 'value', of: 'flag' },
      }),
    );
    expect(answer).toMatchObject({
      problem: { kind: 'wrongPlace', needs: 'list', found: { kind: 'value', of: 'flag' } },
    });
  });
});

describe('a document or an answer that is not what the host sends', () => {
  it('is refused where a kind of argument is not one of the contract', () => {
    const error = refusal(
      decodeDocumentState,
      replaced(state, ['document', 'steps', 0, 'call', 'arguments', 0, 'value'], {
        kind: 'matrix',
      }),
    );
    expect(error.path).toBe('$.document.steps[0].call.arguments[0].value.kind');
    expect(error.message).toContain('"matrix"');
  });

  it('is refused where a number is sent as a number and not as the words the engine shows it as', () => {
    const error = refusal(
      decodeDocumentState,
      replaced(state, ['document', 'steps', 0, 'call', 'arguments', 0, 'value'], {
        kind: 'integer',
        value: 3,
      }),
    );
    expect(error.message).toBe(
      '$.document.steps[0].call.arguments[0].value: is missing the field "shown"',
    );
  });

  it('is refused where the revision is missing or is not a count', () => {
    expect(refusal(decodeDocumentState, without(state, [], 'revision')).message).toBe(
      '$: is missing the field "revision"',
    );
    expect(refusal(decodeDocumentState, replaced(state, ['revision'], -1)).message).toBe(
      '$.revision: must be a whole number from 0, not -1',
    );
  });

  it('is refused where an answer has a status the host does not have', () => {
    expect(refusal(decodeEditAnswer, replaced(edited, ['status'], 'maybe')).message).toBe(
      '$.status: must be one of "edited", "refused", not "maybe"',
    );
  });

  it('is refused where a refusal names a kind of thing, a need or a reason the contract lacks', () => {
    const bad = (problem: unknown): DecodeError =>
      refusal(decodeEditAnswer, replaced(refused, ['problem'], problem));
    expect(bad({ kind: 'stuck' }).path).toBe('$.problem.kind');
    expect(bad({ kind: 'doesNotFit', found: { kind: 'value', of: 'bread' } }).path).toBe(
      '$.problem.found.of',
    );
    expect(bad({ kind: 'wrongPlace', needs: 'bread', found: { kind: 'step' } }).path).toBe(
      '$.problem.needs',
    );
    expect(bad({ kind: 'noStep' }).message).toBe('$.problem: is missing the field "steps"');
    expect(refusal(decodeEditAnswer, withField(refused, [], 'extra', 1)).message).toBe(
      '$: has the field "extra", which the contract lacks',
    );
  });
});
