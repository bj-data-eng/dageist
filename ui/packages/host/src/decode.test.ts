import { describe, expect, it } from 'vitest';

import {
  DecodeError,
  ROOT,
  readBoolean,
  readCount,
  readFields,
  readInteger,
  readList,
  readNullable,
  readNumber,
  readRecord,
  readString,
  readTagged,
  readWord,
} from './decode';
import type { Decode } from './decode';

/** What decoding `value` with `decode` was refused with. */
function refusal<T>(decode: Decode<T>, value: unknown): DecodeError {
  try {
    decode(value, ROOT);
  } catch (error) {
    if (error instanceof DecodeError) return error;
    throw error;
  }
  throw new Error('the value was not refused');
}

describe('the readers of single values', () => {
  it('read a value of the right kind as it is', () => {
    expect(readString('Int64', ROOT)).toBe('Int64');
    expect(readBoolean(false, ROOT)).toBe(false);
    expect(readNumber(-2.5, ROOT)).toBe(-2.5);
    expect(readInteger(-3, ROOT)).toBe(-3);
    expect(readCount(0, ROOT)).toBe(0);
  });

  it('refuse a value of another kind, saying what it is', () => {
    expect(refusal(readString, 7).message).toBe('$: must be a string, not a number');
    expect(refusal(readString, null).message).toBe('$: must be a string, not null');
    expect(refusal(readString, ['a']).message).toBe('$: must be a string, not a list');
    expect(refusal(readBoolean, 'true').message).toBe('$: must be a boolean, not a string');
    expect(refusal(readNumber, { value: 1 }).message).toBe('$: must be a number, not an object');
    expect(refusal(readNumber, undefined).message).toBe('$: must be a number, not undefined');
  });

  it('refuse a number that is not finite, and a count that is not a whole number from 0', () => {
    expect(refusal(readNumber, Number.POSITIVE_INFINITY).problem).toBe('must be a finite number');
    expect(refusal(readCount, -1).problem).toBe('must be a whole number from 0, not -1');
    expect(refusal(readCount, 1.5).problem).toBe('must be a whole number from 0, not 1.5');
    expect(refusal(readCount, '1').problem).toBe('must be a whole number, not a string');
    expect(refusal(readInteger, 0.25).problem).toBe('must be a whole number, not 0.25');
  });
});

describe('the reader of a list', () => {
  it('reads every item and says which one it refuses', () => {
    const numbers = readList(readNumber);
    expect(numbers([1, 2], ROOT)).toEqual([1, 2]);
    const refused = refusal(numbers, [1, 'two', 3]);
    expect(refused.path).toBe('$[1]');
    expect(refused.message).toBe('$[1]: must be a number, not a string');
    expect(refusal(numbers, { length: 0 }).message).toBe('$: must be a list, not an object');
  });
});

describe('the reader of a value that may be null', () => {
  it('passes null and reads anything else', () => {
    const maybe = readNullable(readString);
    expect(maybe(null, ROOT)).toBeNull();
    expect(maybe('x', ROOT)).toBe('x');
    expect(refusal(maybe, 3).message).toBe('$: must be a string, not a number');
  });
});

describe('the reader of one of some words', () => {
  it('names the words it would have read', () => {
    const stage = readWord(['schema', 'rows', 'empty']);
    expect(stage('rows', ROOT)).toBe('rows');
    expect(refusal(stage, 'plan').problem).toBe(
      'must be one of "schema", "rows", "empty", not "plan"',
    );
    expect(refusal(stage, 2).message).toBe('$: must be a string, not a number');
  });
});

describe('the reader of an object', () => {
  const point = readRecord(['x', 'y'], (fields, path) => ({
    x: readNumber(fields.x, `${path}.x`),
    y: readNumber(fields.y, `${path}.y`),
  }));

  it('reads the fields it is given, in any order', () => {
    expect(point({ y: 2, x: 1 }, ROOT)).toEqual({ x: 1, y: 2 });
  });

  it('refuses a missing field and a field the contract lacks', () => {
    expect(refusal(point, { x: 1 }).message).toBe('$: is missing the field "y"');
    expect(refusal(point, { x: 1, y: 2, z: 3 }).message).toBe(
      '$: has the field "z", which the contract lacks',
    );
  });

  it('refuses what is not an object, and a field of the wrong kind where it is', () => {
    expect(refusal(point, [1, 2]).message).toBe('$: must be an object, not a list');
    expect(refusal(point, null).message).toBe('$: must be an object, not null');
    expect(refusal(point, { x: 1, y: '2' }).message).toBe('$.y: must be a number, not a string');
  });

  it('does not take a field of the prototype for one the object has', () => {
    expect(() => readFields({}, ROOT, ['toString'])).toThrow('is missing the field "toString"');
  });
});

describe('the reader of an object tagged with a word', () => {
  type Shape = { kind: 'circle'; radius: number } | { kind: 'point' };
  const shape = readTagged<Shape>('kind', {
    circle: {
      fields: ['radius'],
      read: (fields, path) => ({
        kind: 'circle',
        radius: readNumber(fields.radius, `${path}.radius`),
      }),
    },
    point: { fields: [], read: () => ({ kind: 'point' }) },
  });

  it('reads each variant with its own fields', () => {
    expect(shape({ kind: 'circle', radius: 2 }, ROOT)).toEqual({ kind: 'circle', radius: 2 });
    expect(shape({ kind: 'point' }, ROOT)).toEqual({ kind: 'point' });
  });

  it('refuses a tag it does not know, naming the ones it does', () => {
    const refused = refusal(shape, { kind: 'square' });
    expect(refused.path).toBe('$.kind');
    expect(refused.problem).toBe('must be one of "circle", "point", not "square"');
  });

  it('refuses a missing tag and a tag that is not a word', () => {
    expect(refusal(shape, { radius: 1 }).message).toBe('$: is missing the field "kind"');
    expect(refusal(shape, { kind: 1 }).message).toBe('$.kind: must be a string, not a number');
  });

  it('refuses the fields of another variant', () => {
    expect(refusal(shape, { kind: 'point', radius: 1 }).message).toBe(
      '$: has the field "radius", which the contract lacks',
    );
    expect(refusal(shape, { kind: 'circle' }).message).toBe('$: is missing the field "radius"');
  });

  it('does not take the name of an inherited member for a tag', () => {
    expect(refusal(shape, { kind: 'toString' }).problem).toBe(
      'must be one of "circle", "point", not "toString"',
    );
  });
});
