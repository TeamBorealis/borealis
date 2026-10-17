import type { MigrationIssue, MigrationIssueCode } from "../types";

/** Where an issue was found: the element (id and source type), the file (path inside the imported set) and line. */
export interface IssueAt { element?: string | null; elementType?: string | null; file?: string | null; line?: number | null }

/** A located conversion issue, shaped like the Rust `MigrationIssue` (every field is always sent). */
export const issue = (severity: MigrationIssue["severity"], code: MigrationIssueCode, message: string, at: IssueAt = {}): MigrationIssue => ({
  severity, code, message, element: at.element ?? null, elementType: at.elementType ?? null, file: at.file ?? null, line: at.line ?? null,
});

/** 1-based line of `offset` in `text`. */
export const lineAt = (text: string, offset: number): number => text.slice(0, offset).split("\n").length;

export interface ScannedElement { id: string; type: string; name: string; line: number; body: string }

const attr = (tag: string, name: string) => new RegExp(`\\b${name}="([^"]*)"`).exec(tag)?.[1];

/** The `<element>` records of an Aurora XML text with their id, type, name, line and inner text. */
export function scanElements(text: string): ScannedElement[] {
  return [...text.matchAll(/<element\s[^>]*>/g)].map((m) => {
    const start = m.index ?? 0;
    const end = text.indexOf("</element>", start);
    return {
      id: attr(m[0], "id") ?? "", type: attr(m[0], "type") ?? "Unknown", name: attr(m[0], "name") ?? "Unnamed",
      line: lineAt(text, start), body: text.slice(start, end < 0 ? undefined : end),
    };
  });
}

/** The issues the mock converter reports for one XML file: nothing to import, and `<spellcasting>` converted with defaults. */
export function xmlIssues(text: string, file: string): MigrationIssue[] {
  const els = scanElements(text);
  if (!els.length) return [issue("error", "file_skipped", "No <element> records were found.", { file, line: 1 })];
  return els.flatMap((e) => {
    const at = e.body.search(/<spellcasting/i);
    if (at < 0) return [];
    return [issue("warning", "unsupported_construct", "Spellcasting blocks were converted with default progression.",
      { element: e.id, elementType: e.type, file, line: e.line + lineAt(e.body, at) - 1 })];
  });
}
