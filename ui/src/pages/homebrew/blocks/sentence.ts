import type { HomebrewBlock, HomebrewBlockField, HomebrewBlockSpec, HomebrewForm } from "../../../api/contract";
import { asExprValue } from "../builders/words";
import type { CreatorEnv } from "../env";
import { statLabel } from "../pickers/StatisticPicker";
import { asRef, asRefs, isEmptyValue } from "./values";

/** Values a sentence names at most (the backend's fallback sentence does the same, hc-wb05). */
const SENTENCE_VALUES = 3;

/** One field's value in words, or null when it has no short wording (ids, nested objects, builders). */
function valueWords(f: HomebrewBlockField, v: unknown, env: CreatorEnv): string | null {
  if (isEmptyValue(v) || asExprValue(v)) return null;
  const choice = (x: string) => f.choices?.find((c) => c.value === x)?.label ?? x;
  switch (f.control) {
    case "text": case "name": case "dice":
      return typeof v === "string" ? v : null;
    case "number":
      return typeof v === "number" || typeof v === "string" ? choice(String(v)) : null;
    case "combo":
      return typeof v === "string" ? choice(v) : null;
    case "statistic":
      return typeof v === "string" ? statLabel(env, v) : null;
    case "chips": case "numbers":
      return Array.isArray(v) ? v.map((x) => choice(String(x))).join(", ") : null;
    case "check":
      return v === true ? f.label.toLowerCase() : null;
    case "reference":
      return asRef(v)?.name || null;
    case "references":
      return asRefs(v).map((r) => r.name).filter(Boolean).join(", ") || null;
    default:
      return null;
  }
}

/**
 * The block as one plain sentence while the backend's sentence is not back yet (spec 6: "From level 5: ..."). The
 * backend writes the real sentence (`blocks::sentence`); this one is "<label> (<up to three values>)".
 */
export function fallbackSentence(spec: HomebrewBlockSpec | undefined, b: HomebrewBlock, env: CreatorEnv): string {
  if (!spec) return b.sentence || "A rule";
  const words = spec.fields.flatMap((f) => { const w = valueWords(f, b.fields[f.key], env); return w ? [w] : []; }).slice(0, SENTENCE_VALUES);
  const level = typeof b.fields.level === "number" && b.fields.level > 1 ? `From level ${b.fields.level}: ` : "";
  const body = words.length ? `${spec.label} (${words.join("; ")})` : spec.label;
  const onlyIf = !isEmptyValue(b.fields.requirements) ? ", only if a condition holds" : "";
  return `${level}${body}${onlyIf}`;
}

/**
 * The sentence to show for a block: the backend's, from the last answer, while the block still holds what was sent;
 * else the block's own (a sentence read from content, or the fallback written when it was edited).
 */
export function shownSentence(b: HomebrewBlock, answer: HomebrewForm | null | undefined): string {
  const sent = answer?.blocks.find((x) => x.key === b.key);
  if (sent && sent.sentence && JSON.stringify(sent.fields) === JSON.stringify(b.fields)) return sent.sentence;
  return b.sentence;
}
