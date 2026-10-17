/**
 * Content the rules-based importers read never goes to the AI importer (user rule: Aurora and 5etools
 * imports are deterministic and never involve AI). Aurora XML (an `<elements>` file, an element
 * fragment or an `.index`), 5etools data and Borealis content packs are recognised by shape and
 * refused with a pointer to Additional Content. Mirrors `src-tauri/src/import_rules_content.rs`,
 * which refuses the same content again before any model is called.
 */

export type RulesContent = "aurora" | "5etools" | "borealis";

/** Top-level arrays of a 5etools data file (`data/*.json`, homebrew). */
const FIVETOOLS_KEYS = new Set([
  "action", "adventure", "background", "baseitem", "book", "boon", "card", "charoption", "class", "classFeature",
  "condition", "cult", "deck", "deity", "disease", "facility", "feat", "hazard", "item", "itemGroup", "itemMastery",
  "itemProperty", "itemType", "language", "legendaryGroup", "magicvariant", "monster", "object", "optionalfeature",
  "psionic", "race", "recipe", "reward", "sense", "skill", "spell", "status", "subclass", "subclassFeature", "subrace",
  "table", "trap", "variantrule", "vehicle",
]);

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

const hasSourcedEntry = (v: unknown): boolean => Array.isArray(v) && v.some((e) => isObj(e) && typeof e.source === "string");

/** One 5etools record: a name, a source abbreviation and its `entries` or page number. */
const isFiveToolsEntity = (v: unknown): boolean =>
  isObj(v) && typeof v.name === "string" && typeof v.source === "string" && (Array.isArray(v.entries) || typeof v.page === "number");

function detectJson(v: unknown): RulesContent | null {
  if (Array.isArray(v)) return v.some(isFiveToolsEntity) ? "5etools" : null;
  if (!isObj(v)) return null;
  if (typeof v.format === "string" && v.format.startsWith("borealis-")) return "borealis";
  const fiveTools = isObj(v._meta)
    || Object.entries(v).some(([k, val]) => (FIVETOOLS_KEYS.has(k) || k.endsWith("Fluff")) && hasSourcedEntry(val))
    || isFiveToolsEntity(v);
  return fiveTools ? "5etools" : null;
}

const tag = (name: string) => new RegExp(`<${name}[\\s>/]`, "i");
const AURORA_ELEMENT = /<element\s[^>]*\sid=["']ID_/i;

function isAuroraXml(t: string): boolean {
  return tag("elements").test(t) || AURORA_ELEMENT.test(t) || (tag("index").test(t) && tag("files").test(t));
}

/** What kind of rules content `text` is, if any. */
export function detectRulesContent(text: string): RulesContent | null {
  const t = text.replace(/^﻿/, "").trim();
  if (t.startsWith("{") || t.startsWith("[")) {
    try {
      return detectJson(JSON.parse(t));
    } catch {
      /* not JSON: fall through to the XML check */
    }
  }
  return t.includes("<") && isAuroraXml(t) ? "aurora" : null;
}

/** The friendly refusal; `subject` is a quoted file name or "The pasted text". Same words as the backend. */
export function rulesContentRefusal(kind: RulesContent, subject: string): string {
  switch (kind) {
    case "aurora":
      return `${subject} is Aurora content. It is added as it is, without AI: save it as an .xml file and add it under Additional Content > Custom content.`;
    case "5etools":
      return `${subject} is 5etools data. It is converted as it is, without AI: use Additional Content > 5etools import with your 5etools data folder.`;
    case "borealis":
      return `${subject} is already a Borealis content pack. Add it as it is under Additional Content > Custom content.`;
  }
}

/** How refusals name pasted or typed text. */
export const PASTED_SUBJECT = "The pasted text";

/** The refusal for `text`, or null when it may go to the AI importer. */
export function rulesContentProblem(text: string, subject: string): string | null {
  const kind = detectRulesContent(text);
  return kind ? rulesContentRefusal(kind, subject) : null;
}
