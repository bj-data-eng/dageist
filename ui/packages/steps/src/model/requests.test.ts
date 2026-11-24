import { describe, expect, it } from 'vitest';

import type { KindView, LiteralView } from '@dageist/host';

import {
  HOLE,
  canBeLeftUnpicked,
  fieldArgument,
  flagArgument,
  inputArgument,
  listArgument,
  listOfOne,
  literalArgument,
  literalExpression,
  wordArgument,
} from './requests';

const integer = (value: number): LiteralView => ({ kind: 'integer', value, shown: String(value) });
const float = (value: number): LiteralView => ({ kind: 'float', value, shown: String(value) });
const text = (value: string): LiteralView => ({ kind: 'text', value, shown: `"${value}"` });

const valueOf = (type: 'integer' | 'float' | 'string' | 'date' | 'any'): KindView => ({
  kind: 'value',
  type,
});

describe('a typed literal as the argument of a parameter', () => {
  it('is a one-word expression where the parameter takes an expression', () => {
    expect(literalArgument({ kind: 'expression' }, integer(3))).toEqual({
      kind: 'expression',
      expression: { kind: 'literal', literal: { kind: 'integer', value: 3 } },
    });
    expect(literalExpression(float(1.5))).toEqual({
      kind: 'expression',
      expression: { kind: 'literal', literal: { kind: 'float', value: 1.5 } },
    });
    expect(literalExpression(text('x'))).toEqual({
      kind: 'expression',
      expression: { kind: 'literal', literal: { kind: 'text', value: 'x' } },
    });
  });

  it('is a number of the parameter’s own type where it takes an integer or a float', () => {
    expect(literalArgument(valueOf('integer'), integer(7))).toEqual({ kind: 'integer', value: 7 });
    expect(literalArgument(valueOf('float'), integer(7))).toEqual({ kind: 'float', value: 7 });
    expect(literalArgument(valueOf('float'), float(2.5))).toEqual({ kind: 'float', value: 2.5 });
  });

  it('is no request where text is given for a parameter that takes a number', () => {
    // There is no number in it to send, and a NaN is not one the engine could read.
    expect(literalArgument(valueOf('integer'), text('abc'))).toBeNull();
    expect(literalArgument(valueOf('float'), text('abc'))).toBeNull();
    expect(literalArgument(valueOf('integer'), integer(Number.NaN))).toBeNull();
    expect(literalArgument(valueOf('any'), float(Number.NaN))).toBeNull();
  });

  it('is text where the parameter takes a string, written as the literal shows it if it is a number', () => {
    expect(literalArgument(valueOf('string'), text('Company A'))).toEqual({
      kind: 'string',
      value: 'Company A',
    });
    expect(literalArgument(valueOf('string'), integer(12))).toEqual({
      kind: 'string',
      value: '12',
    });
  });

  it('keeps its own kind where the parameter takes a value of any type', () => {
    expect(literalArgument(valueOf('any'), text('a'))).toEqual({ kind: 'string', value: 'a' });
    expect(literalArgument(valueOf('any'), integer(4))).toEqual({ kind: 'integer', value: 4 });
    expect(literalArgument(valueOf('any'), float(4.5))).toEqual({ kind: 'float', value: 4.5 });
    // A float literal stays a float even where it is whole.
    expect(literalArgument(valueOf('any'), float(4))).toEqual({ kind: 'float', value: 4 });
    // A whole number too large to be held exactly is sent as a float.
    expect(literalArgument(valueOf('any'), integer(2 ** 60))).toEqual({
      kind: 'float',
      value: 2 ** 60,
    });
  });

  it('is the text where the parameter takes a duration, a pattern, a time zone or a path', () => {
    expect(literalArgument({ kind: 'duration' }, text('1d'))).toEqual({
      kind: 'duration',
      text: '1d',
    });
    expect(literalArgument({ kind: 'pattern', plainTextFlag: null }, text('a.*'))).toEqual({
      kind: 'pattern',
      text: 'a.*',
    });
    expect(literalArgument({ kind: 'timeZone', none: null }, text('UTC'))).toEqual({
      kind: 'timeZone',
      zone: 'UTC',
    });
    expect(literalArgument({ kind: 'path' }, text('a/b'))).toEqual({ kind: 'path', text: 'a/b' });
  });

  it('is a string for any other kind', () => {
    expect(literalArgument({ kind: 'flag' }, text('yes'))).toEqual({
      kind: 'string',
      value: 'yes',
    });
  });
});

describe('a picked word as the argument of a parameter', () => {
  it('is a choice, a dtype, a column or a table by the parameter’s kind', () => {
    expect(wordArgument({ kind: 'choice', words: [] }, 'left')).toEqual({
      kind: 'choice',
      word: 'left',
    });
    expect(wordArgument({ kind: 'dtype' }, 'Int8')).toEqual({ kind: 'dtype', name: 'Int8' });
    expect(wordArgument({ kind: 'column' }, 'dcn')).toEqual({ kind: 'columns', names: ['dcn'] });
    expect(wordArgument({ kind: 'table' }, 'step_sla')).toEqual({
      kind: 'table',
      name: 'step_sla',
    });
  });

  it('is no time zone for a time zone, and a string for any other kind', () => {
    expect(wordArgument({ kind: 'timeZone', none: null }, 'none')).toEqual({
      kind: 'timeZone',
      zone: null,
    });
    expect(wordArgument({ kind: 'path' }, 'x')).toEqual({ kind: 'string', value: 'x' });
  });
});

describe('the other arguments the editor builds', () => {
  it('are a hole, a field, an input, a flag and lists', () => {
    expect(HOLE).toEqual({ kind: 'expression', expression: { kind: 'hole' } });
    expect(fieldArgument('dcn')).toEqual({
      kind: 'expression',
      expression: { kind: 'field', name: 'dcn' },
    });
    expect(inputArgument('holidays')).toEqual({
      kind: 'expression',
      expression: { kind: 'input', name: 'holidays' },
    });
    expect(flagArgument(false)).toEqual({ kind: 'flag', value: false });
    expect(listArgument([HOLE, fieldArgument('a')])).toEqual({
      kind: 'many',
      items: [HOLE, fieldArgument('a')],
    });
    expect(listOfOne(flagArgument(true))).toEqual({ kind: 'many', items: [flagArgument(true)] });
  });

  it('can be left unpicked only for an expression', () => {
    expect(canBeLeftUnpicked({ kind: 'expression' })).toBe(true);
    expect(canBeLeftUnpicked({ kind: 'column' })).toBe(false);
    expect(canBeLeftUnpicked(valueOf('integer'))).toBe(false);
  });
});
