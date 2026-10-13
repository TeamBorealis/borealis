/**
 * Exact comparison with a readable label. Whitespace and case are ignored so uppercase UI text
 * ("OK|CANCEL") and mixed case compare equal. Throws on mismatch.
 */
export function expectEqual(name: string, expected: number | string, actual: number | string): void {
  const norm = (s: number | string) => String(s).replace(/\s+/g, '').toLowerCase();
  if (norm(expected) !== norm(actual)) throw new Error(`mismatch ${name}: expected ${expected}, got ${actual}`);
}
