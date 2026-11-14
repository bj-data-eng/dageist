let issued = 0;

/** Tells the identifiers made in this window from those made in another window. */
const WINDOW = Math.random().toString(36).slice(2, 8);

/** Returns an identifier no other step or method has, in this window or any other. */
export function nextId(prefix: string): string {
  issued += 1;
  return `${prefix}-${WINDOW}-${String(issued)}`;
}
