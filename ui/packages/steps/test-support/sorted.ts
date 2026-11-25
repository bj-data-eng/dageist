/**
 * @file Test support: puts words in order, so that a test compares lists without caring which
 * came first. It is not part of the package's front door.
 */

/** Returns `words` in alphabetical order, leaving `words` as it is. */
export function sorted(words: readonly string[]): string[] {
  const copy = [...words];
  copy.sort((left, right) => left.localeCompare(right));
  return copy;
}
