let issued = 0;

/** Returns an identifier no other step or method in this session has. */
export function nextId(prefix: string): string {
  issued += 1;
  return `${prefix}-${String(issued)}`;
}
