import { parseKey } from 'e2e/engine';

/** ANSI virtual key codes (US layout) for printable characters. */
const CHAR_CODES: Record<string, number> = {
  a: 0, s: 1, d: 2, f: 3, h: 4, g: 5, z: 6, x: 7, c: 8, v: 9, b: 11, q: 12, w: 13, e: 14, r: 15, y: 16, t: 17,
  '1': 18, '2': 19, '3': 20, '4': 21, '6': 22, '5': 23, '=': 24, '9': 25, '7': 26, '-': 27, '8': 28, '0': 29,
  ']': 30, o: 31, u: 32, '[': 33, i: 34, p: 35, l: 37, j: 38, "'": 39, k: 40, ';': 41, '\\': 42, ',': 43,
  '/': 44, n: 45, m: 46, '.': 47, '`': 50,
};
const NAMED_CODES: Record<string, number> = {
  Enter: 36, Tab: 48, Space: 49, Backspace: 51, Escape: 53, Delete: 117, Home: 115, End: 119, PageUp: 116, PageDown: 121,
  ArrowLeft: 123, ArrowRight: 124, ArrowDown: 125, ArrowUp: 126,
  F1: 122, F2: 120, F3: 99, F4: 118, F5: 96, F6: 97, F7: 98, F8: 100, F9: 101, F10: 109, F11: 103, F12: 111,
};
const MODIFIER_FLAGS: Record<string, string> = { Shift: 'shift', Control: 'control', Alt: 'option', Meta: 'command', ControlOrMeta: 'command' };

export interface KeyChord { readonly code: number; readonly modifiers: readonly string[] }

/** Spelling like `ControlOrMeta+s` to a macOS key code and modifier flags (ControlOrMeta is Cmd here). Undefined when unsendable. */
export function toChord(key: string): KeyChord | undefined {
  const parsed = parseKey(key);
  if (parsed === undefined) return undefined;
  const modifiers = parsed.modifiers.map((m) => MODIFIER_FLAGS[m]).filter((m): m is string => m !== undefined);
  if (parsed.key.kind === 'named') {
    const code = NAMED_CODES[parsed.key.name];
    return code === undefined ? undefined : { code, modifiers };
  }
  const char = parsed.key.char;
  const code = CHAR_CODES[char.toLowerCase()];
  if (code === undefined) return undefined;
  const shifted = char !== char.toLowerCase() && !modifiers.includes('shift');
  return { code, modifiers: shifted ? [...modifiers, 'shift'] : modifiers };
}
