/** Joins class names, leaving out the ones that are switched off. */
export function cx(...parts: ReadonlyArray<string | false | null | undefined>): string {
  return parts.filter((part) => typeof part === 'string' && part !== '').join(' ');
}
