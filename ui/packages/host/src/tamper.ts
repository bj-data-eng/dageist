/**
 * @file Test support: copies of a fixture with one thing changed, to see what
 * a decoder says of a value that is not what the host sends. It is not part
 * of the package's front door.
 */

/** One step down into a value: a field of an object, or an item of a list. */
export type Step = string | number;

function isList(value: unknown): value is readonly unknown[] {
  return Array.isArray(value);
}

function isObject(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** A copy of `value` with what is at `path` changed by `change`. */
function changed(
  value: unknown,
  path: readonly Step[],
  change: (found: unknown) => unknown,
): unknown {
  const [head, ...rest] = path;
  if (head === undefined) return change(value);
  if (isList(value) && typeof head === 'number') {
    return value.map((item, index) => (index === head ? changed(item, rest, change) : item));
  }
  if (isObject(value) && typeof head === 'string') {
    return { ...value, [head]: changed(value[head], rest, change) };
  }
  throw new Error(`there is nothing at ${String(head)} to change`);
}

/** A copy of `value` with `replacement` where `path` leads. */
export function replaced(value: unknown, path: readonly Step[], replacement: unknown): unknown {
  return changed(value, path, () => replacement);
}

/** A copy of `value` without the field `name` of the object `path` leads to. */
export function without(value: unknown, path: readonly Step[], name: string): unknown {
  return changed(value, path, (found) => {
    if (!isObject(found)) throw new Error('there is no object to take a field from');
    return Object.fromEntries(Object.entries(found).filter(([key]) => key !== name));
  });
}

/** A copy of `value` with the field `name` set to `field` in the object `path` leads to. */
export function withField(
  value: unknown,
  path: readonly Step[],
  name: string,
  field: unknown,
): unknown {
  return changed(value, path, (found) => {
    if (!isObject(found)) throw new Error('there is no object to add a field to');
    return { ...found, [name]: field };
  });
}

/** What is at `path` in `value`. */
export function at(value: unknown, path: readonly Step[]): unknown {
  let found = value;
  for (const step of path) {
    if (isList(found) && typeof step === 'number') {
      found = found[step];
    } else if (isObject(found) && typeof step === 'string') {
      found = found[step];
    } else {
      throw new Error(`there is nothing at ${String(step)}`);
    }
  }
  return found;
}
