import type { EscapeKind, TextFinding } from "./types";

// Text rules shared with the Rust twin (`borealis_content::rich::{lint, normalize}`); the shared fixtures
// (`crates/borealis-content/tests/fixtures/rich_text.json`) keep both in step. Spec 7.2-7.3.

/** Named character references both sides decode (`entities.rs::NAMED`). */
const NAMED: Readonly<Record<string, string>> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: "\u{a0}",
  rsquo: "\u{2019}",
  lsquo: "\u{2018}",
  rdquo: "\u{201d}",
  ldquo: "\u{201c}",
  sbquo: "\u{201a}",
  bdquo: "\u{201e}",
  ndash: "\u{2013}",
  mdash: "\u{2014}",
  hellip: "\u{2026}",
  minus: "\u{2212}",
  times: "\u{d7}",
  divide: "\u{f7}",
  frac12: "\u{bd}",
  frac14: "\u{bc}",
  frac34: "\u{be}",
  deg: "\u{b0}",
  plusmn: "\u{b1}",
  copy: "\u{a9}",
  reg: "\u{ae}",
  trade: "\u{2122}",
  bull: "\u{2022}",
  middot: "\u{b7}",
  eacute: "\u{e9}",
  prime: "\u{2032}",
  Prime: "\u{2033}",
};

/** Longest reference body looked at, in characters. */
const MAX_ENTITY_BODY = 10;
const MAX_FIX_PASSES = 8;
const NBSP = "\u{a0}";

/** Windows-1252 bytes 0x80..0x9F; the five undefined bytes map to the C1 control of the same number. */
const CP1252_HIGH = [
  0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f,
  0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
];

/** Rust's `char::is_whitespace` (Unicode White_Space); JS `\s` differs on U+0085 and U+FEFF. */
const WHITE_SPACE = /[\t\n\v\f\r \u{85}\u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}]/u;
const WHITE_SPACE_RUNS = /[\t\n\v\f\r \u{85}\u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}]+/gu;
const CONTROL = /\p{Cc}/u;

/** Decode the character reference at the start of `s` (which starts with `&`): the character and its length. */
export function decodeEntityAt(s: string): [string, number] | null {
  if (!s.startsWith("&")) return null;
  const body = s.slice(1);
  let end = -1;
  let seen = 0;
  for (let i = 0; i < body.length && seen <= MAX_ENTITY_BODY; seen += 1) {
    const ch = String.fromCodePoint(body.codePointAt(i) ?? 0);
    if (ch === ";") {
      end = i;
      break;
    }
    i += ch.length;
  }
  if (end < 0) return null;
  const name = body.slice(0, end);
  let decoded: string | undefined;
  if (name.startsWith("#")) {
    const hex = name[1] === "x" || name[1] === "X";
    const digits = name.slice(hex ? 2 : 1);
    if (!(hex ? /^[0-9a-fA-F]+$/ : /^[0-9]+$/).test(digits)) return null;
    const code = parseInt(digits, hex ? 16 : 10);
    if (code === 0 || code > 0x10ffff || (code >= 0xd800 && code < 0xe000)) return null;
    decoded = String.fromCodePoint(code);
  } else if (Object.prototype.hasOwnProperty.call(NAMED, name)) {
    decoded = NAMED[name];
  }
  return decoded === undefined ? null : [decoded, end + 2];
}

const SIMPLE_ESCAPES: Readonly<Record<string, string>> = { n: "\n", r: "", t: " ", '"': '"', "'": "'", "/": "/" };

function backslashAt(rest: string): [EscapeKind, number, string] {
  let run = 0;
  while (rest[run] === "\\") run += 1;
  const after = rest.slice(run);
  const simple = after[0] === undefined ? undefined : SIMPLE_ESCAPES[after[0]];
  if (simple !== undefined) return ["backslash_escape", run + 1, simple];
  const unicode = unicodeEscape(after);
  if (unicode) return ["backslash_escape", run + unicode[0], unicode[1]];
  return ["stray_backslash", run, ""];
}

function hex4(s: string): number | null {
  const m = /^u([0-9a-fA-F]{4})/.exec(s);
  return m?.[1] === undefined ? null : parseInt(m[1], 16);
}

/** `uXXXX` (or a surrogate pair `uD83D` + backslash + `uDE00`) at the start of `s`: length and character. */
function unicodeEscape(s: string): [number, string] | null {
  const first = hex4(s);
  if (first === null) return null;
  if (first >= 0xd800 && first < 0xdc00 && s[5] === "\\") {
    const low = hex4(s.slice(6));
    if (low !== null && low >= 0xdc00 && low < 0xe000) {
      return [11, String.fromCodePoint(0x10000 + ((first - 0xd800) << 10) + (low - 0xdc00))];
    }
  }
  const lone = first === 0 || (first >= 0xd800 && first < 0xe000);
  return [5, lone ? "" : String.fromCodePoint(first)];
}

function cp1252Byte(code: number): number | null {
  if (code >= 0x80 && code <= 0xff) return code;
  const i = CP1252_HIGH.indexOf(code);
  return i < 0 ? null : 0x80 + i;
}

const UTF8 = new TextDecoder("utf-8", { fatal: true });

/** UTF-8 bytes shown as Windows-1252 characters at the start of `s`: length in `s` and the real character. */
function mojibakeAt(s: string): [number, string] | null {
  const chars = Array.from(s.slice(0, 8));
  const lead = chars[0] === undefined ? null : cp1252Byte(chars[0].codePointAt(0) ?? 0);
  if (lead === null) return null;
  const continuation = lead >= 0xc2 && lead <= 0xdf ? 1 : lead >= 0xe0 && lead <= 0xef ? 2 : lead >= 0xf0 && lead <= 0xf4 ? 3 : 0;
  if (continuation === 0) return null;
  const bytes = [lead];
  let length = chars[0]?.length ?? 0;
  for (let k = 1; k <= continuation; k += 1) {
    const ch = chars[k];
    const b = ch === undefined ? null : cp1252Byte(ch.codePointAt(0) ?? 0);
    if (ch === undefined || b === null || b < 0x80 || b > 0xbf) return null;
    bytes.push(b);
    length += ch.length;
  }
  try {
    const decoded = UTF8.decode(new Uint8Array(bytes));
    return decoded ? [length, Array.from(decoded)[0] ?? ""] : null;
  } catch {
    return null;
  }
}

/** Find escape artefacts in `text` (spec 7.3): backslash escapes, character references shown as text, mojibake. */
export function lintText(text: string): TextFinding[] {
  const out: TextFinding[] = [];
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    const ch = String.fromCodePoint(rest.codePointAt(0) ?? 0);
    let hit: [EscapeKind, number, string] | null = null;
    if (ch === "\\") {
      hit = backslashAt(rest);
    } else if (ch === "&") {
      const entity = decodeEntityAt(rest);
      if (entity) hit = ["html_entity", entity[1], entity[0] === NBSP ? " " : entity[0]];
    } else {
      const moji = mojibakeAt(rest);
      if (moji) hit = ["mojibake", moji[0], moji[1]];
    }
    if (hit) {
      out.push({ kind: hit[0], start: i, found: rest.slice(0, hit[1]), replacement: hit[2] });
      i += hit[1];
    } else {
      i += ch.length;
    }
  }
  return out;
}

/** Replace every artefact in `text`, repeating until none is left (`&amp;amp;` ends as `&`). */
export function fixText(text: string): string {
  let current = text;
  for (let pass = 0; pass < MAX_FIX_PASSES; pass += 1) {
    const findings = lintText(current);
    if (findings.length === 0) break;
    let next = "";
    let at = 0;
    for (const f of findings) {
      next += current.slice(at, f.start) + f.replacement;
      at = f.start + f.found.length;
    }
    current = next + current.slice(at);
  }
  return current;
}

/** Text of a rich-text node: NFC, controls other than tab and line feed removed, no-break spaces become spaces
 * (except in table cells). */
export function normalizeRichText(text: string, inCell: boolean): string {
  let out = "";
  for (const ch of text.normalize("NFC")) {
    if (CONTROL.test(ch) && ch !== "\n" && ch !== "\t") continue;
    out += ch === NBSP && !inCell ? " " : ch;
  }
  return out;
}

/** A plain text field (names, short texts): NFC, controls removed, white space runs one space, trimmed. */
export function normalizePlainText(text: string): string {
  let kept = "";
  for (const ch of text.normalize("NFC")) {
    if (CONTROL.test(ch) && !WHITE_SPACE.test(ch)) continue;
    kept += ch;
  }
  return kept.split(WHITE_SPACE_RUNS).filter((part) => part !== "").join(" ");
}
