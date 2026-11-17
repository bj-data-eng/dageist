/**
 * @file The readers every decoder is made of. The host sends JSON, which
 * TypeScript can only call `unknown`. A decoder checks the value against the
 * contract and returns it typed, or refuses it with the place and the reason,
 * so a change of shape on the host's side fails where it is read and not
 * somewhere far from it.
 *
 * Objects are closed: an object with a field the contract lacks, or without
 * one it has, is refused, as is a tag that is not one of the contract's.
 */

/** A value that does not have the shape the contract names, and where. */
export class DecodeError extends Error {
  /**
   * @param path Where in the value, written as `$.steps[2].outcome`.
   * @param problem What is wrong there.
   */
  constructor(
    readonly path: string,
    readonly problem: string,
  ) {
    super(`${path}: ${problem}`);
    this.name = 'DecodeError';
  }
}

/** Reads `value`, found at `path`, as a `T`, or refuses it with a `DecodeError`. */
export type Decode<T> = (value: unknown, path: string) => T;

/** The fields of a JSON object, each one still unread. */
export type Fields = Readonly<Record<string, unknown>>;

/** The place the whole value is found at. */
export const ROOT = '$';

/** The place of the field `name` of the object at `path`. */
export function fieldPath(path: string, name: string): string {
  return `${path}.${name}`;
}

/** The place of the item `index` of the list at `path`. */
export function itemPath(path: string, index: number): string {
  return `${path}[${String(index)}]`;
}

function isObject(value: unknown): value is Fields {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isList(value: unknown): value is readonly unknown[] {
  return Array.isArray(value);
}

/** What `value` is, as a phrase for a reason. */
function kindOf(value: unknown): string {
  if (value === null) return 'null';
  if (isList(value)) return 'a list';
  switch (typeof value) {
    case 'string':
      return 'a string';
    case 'number':
      return 'a number';
    case 'boolean':
      return 'a boolean';
    case 'object':
      return 'an object';
    default:
      return typeof value;
  }
}

function refuseShape(path: string, expected: string, value: unknown): never {
  throw new DecodeError(path, `must be ${expected}, not ${kindOf(value)}`);
}

/** Reads a string. */
export const readString: Decode<string> = (value, path) => {
  if (typeof value !== 'string') return refuseShape(path, 'a string', value);
  return value;
};

/** Reads a Boolean. */
export const readBoolean: Decode<boolean> = (value, path) => {
  if (typeof value !== 'boolean') return refuseShape(path, 'a boolean', value);
  return value;
};

/** Reads a finite number. */
export const readNumber: Decode<number> = (value, path) => {
  if (typeof value !== 'number') return refuseShape(path, 'a number', value);
  if (!Number.isFinite(value)) throw new DecodeError(path, 'must be a finite number');
  return value;
};

/** Reads a whole number, as a count or a position counts: not negative. */
export const readCount: Decode<number> = (value, path) => {
  if (typeof value !== 'number') return refuseShape(path, 'a whole number', value);
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new DecodeError(path, `must be a whole number from 0, not ${String(value)}`);
  }
  return value;
};

/** Reads a whole number that may be negative. */
export const readInteger: Decode<number> = (value, path) => {
  if (typeof value !== 'number') return refuseShape(path, 'a whole number', value);
  if (!Number.isSafeInteger(value)) {
    throw new DecodeError(path, `must be a whole number, not ${String(value)}`);
  }
  return value;
};

/** Reads a list, each of whose items `each` reads. */
export function readList<T>(each: Decode<T>): Decode<readonly T[]> {
  return (value, path) => {
    if (!isList(value)) return refuseShape(path, 'a list', value);
    return value.map((item, index) => each(item, itemPath(path, index)));
  };
}

/** Reads `null`, or what `each` reads. */
export function readNullable<T>(each: Decode<T>): Decode<T | null> {
  return (value, path) => (value === null ? null : each(value, path));
}

/** Reads one of `words`. */
export function readWord<W extends string>(words: readonly W[]): Decode<W> {
  return (value, path) => {
    const text = readString(value, path);
    const found = words.find((word) => word === text);
    if (found === undefined) {
      throw new DecodeError(
        path,
        `must be one of ${words.map((word) => JSON.stringify(word)).join(', ')}, not ${JSON.stringify(text)}`,
      );
    }
    return found;
  };
}

/** Reads an object that has exactly the fields `names`. */
export function readFields(value: unknown, path: string, names: readonly string[]): Fields {
  if (!isObject(value)) return refuseShape(path, 'an object', value);
  for (const name of names) {
    if (!Object.hasOwn(value, name)) {
      throw new DecodeError(path, `is missing the field ${JSON.stringify(name)}`);
    }
  }
  for (const name of Object.keys(value)) {
    if (!names.includes(name)) {
      throw new DecodeError(
        path,
        `has the field ${JSON.stringify(name)}, which the contract lacks`,
      );
    }
  }
  return value;
}

/** Reads the field `name` of `fields`, found under `path`, with `each`. */
export function readField<T>(fields: Fields, path: string, name: string, each: Decode<T>): T {
  return each(fields[name], fieldPath(path, name));
}

/** One way an object tagged with a word can be, and how it is read. */
export interface Variant<T> {
  /** The fields it has besides the tag. */
  readonly fields: readonly string[];
  /** Reads it, once its fields are known to be exactly those. */
  readonly read: (fields: Fields, path: string) => T;
}

/**
 * Reads an object whose field `tag` names which of `variants` it is, and
 * which then has exactly that variant's fields.
 */
export function readTagged<T>(
  tag: string,
  variants: Readonly<Record<string, Variant<T>>>,
): Decode<T> {
  const names = Object.keys(variants);
  return (value, path) => {
    if (!isObject(value)) return refuseShape(path, 'an object', value);
    if (!Object.hasOwn(value, tag)) {
      throw new DecodeError(path, `is missing the field ${JSON.stringify(tag)}`);
    }
    const word = readWord(names)(value[tag], fieldPath(path, tag));
    const variant = variants[word];
    if (variant === undefined) {
      throw new DecodeError(path, `has no variant ${JSON.stringify(word)}`);
    }
    return variant.read(readFields(value, path, [tag, ...variant.fields]), path);
  };
}

/** Reads an object that has exactly the fields `names`, with `read`. */
export function readRecord<T>(
  names: readonly string[],
  read: (fields: Fields, path: string) => T,
): Decode<T> {
  return (value, path) => read(readFields(value, path, names), path);
}
