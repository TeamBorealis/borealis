import type { CitedAnswerData, CitedSource } from "../../api/contract";

/** A run of answer text, or an inline `[[ELEMENT_ID]]` citation marker. */
export type AnswerPart = { kind: "text"; text: string } | { kind: "cite"; id: string };

const MARKER = /\[\[([^\]\s]+)\]\]/g;
const CONFIDENCE = new Set(["high", "medium", "low"]);

/** Split answer text at its citation markers, in order. */
export function splitCitations(text: string): AnswerPart[] {
  const parts: AnswerPart[] = [];
  let last = 0;
  for (const m of text.matchAll(MARKER)) {
    const at = m.index ?? 0;
    if (at > last) parts.push({ kind: "text", text: text.slice(last, at) });
    parts.push({ kind: "cite", id: m[1]! });
    last = at + m[0].length;
  }
  if (last < text.length) parts.push({ kind: "text", text: text.slice(last) });
  return parts;
}

const isString = (v: unknown): v is string => typeof v === "string";
const isStringArray = (v: unknown): v is string[] => Array.isArray(v) && v.every(isString);

function isSource(v: unknown): v is CitedSource {
  if (typeof v !== "object" || v === null) return false;
  const s = v as Record<string, unknown>;
  return isString(s.id) && isString(s.name) && isString(s.type);
}

/** Validate a `rules_qa` result payload from the backend; `null` when it does not have the expected shape. */
export function asCitedAnswer(data: unknown): CitedAnswerData | null {
  if (typeof data !== "object" || data === null) return null;
  const d = data as Record<string, unknown>;
  const ok = isString(d.answerMarkdown) && isStringArray(d.citations) && Array.isArray(d.sources) && d.sources.every(isSource)
    && isString(d.confidence) && CONFIDENCE.has(d.confidence) && typeof d.notCovered === "boolean"
    && isStringArray(d.strippedCitations) && isStringArray(d.uncitedClaims) && typeof d.unverified === "boolean";
  return ok ? (d as unknown as CitedAnswerData) : null;
}
