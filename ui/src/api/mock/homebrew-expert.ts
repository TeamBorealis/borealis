import type {
  ElementSummary, HomebrewDataNode, HomebrewElementRef, HomebrewForm, HomebrewGeneratedView, HomebrewSpellListRow, HomebrewUseRelation,
} from "../types";
import { pickerRow } from "./homebrew-check";

// The web mock's stand-ins for WB-23 (spec 12): where used, SPELL LISTS rows, the DATA tree and the list filter's fields.
// The real answers come from `borealis_authoring::pack::{use_of, with_spell_list}` and `form::data_tree`.

const str = (v: unknown): string => (typeof v === "string" ? v.trim() : "");
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);

/** A form's tags: its Tags chips, or a spell's lists (every support tag, WB-11). */
export function tagsOf(form: HomebrewForm): string[] {
  return strings(form.kind === "spell" ? form.fields.spellLists : form.fields.tags);
}

/** The new tags of a form: `tags` (or a spell's lists) with `add` added once and `remove` taken out (any case). */
export function withTags(form: HomebrewForm, add: readonly string[], remove: readonly string[]): HomebrewForm {
  const key = form.kind === "spell" ? "spellLists" : "tags";
  const now = tagsOf(form);
  const kept = now.filter((t) => !remove.some((r) => r.trim().toLowerCase() === t.toLowerCase()));
  const next = [...kept, ...add.map((t) => t.trim()).filter((t) => t && !kept.some((k) => k.toLowerCase() === t.toLowerCase()))];
  return next.join("\u0000") === now.join("\u0000") ? form : { ...form, fields: { ...form.fields, [key]: next } };
}

interface PmNode { type: string; text?: string; content?: PmNode[] }
const plain = (n: PmNode | null | undefined): string => (n ? (n.text ?? "") + (n.content ?? []).map(plain).join(" ") : "");

/** The list filter's text of a form: its summary line and description as plain text, cut short (backend: 400). */
export function textOf(form: HomebrewForm): string {
  const d = form.fields.description;
  const body = typeof d === "string" ? d : plain(d as PmNode);
  return `${str(form.fields.summary)} ${body}`.replace(/\s+/g, " ").trim().slice(0, 400);
}

const leaf = (key: string, kind: HomebrewDataNode["kind"], text: string): HomebrewDataNode => ({ key, kind, text });

/** The DATA view of a form (spec 12.1): the element the mock would store, as the backend's tree and JSON. */
export function generatedOf(form: HomebrewForm, id: string, type: string, source: string, descriptionHtml: string): HomebrewGeneratedView {
  const name = str(form.fields.name);
  const tags = tagsOf(form);
  const element: Record<string, unknown> = { id, name, type, source, ...(tags.length ? { supports: tags } : {}), ...(descriptionHtml ? { description: descriptionHtml } : {}) };
  const tree: HomebrewDataNode[] = [{
    key: name, kind: "object",
    children: [
      leaf("id", "text", id), leaf("name", "text", name), leaf("type", "text", type), leaf("source", "text", source),
      ...(tags.length ? [{ key: "supports", kind: "list" as const, children: tags.map((t, i) => leaf(String(i + 1), "text", t)) }] : []),
      ...(descriptionHtml ? [leaf("description", "html", descriptionHtml)] : []),
    ],
  }];
  return { json: `${JSON.stringify(element, null, 2)}\n`, tree };
}

/** The DATA view of a copy (H10ac): the element the real backend wrote for it (`homebrew-copies.ts`), with the draft's
 * own id, name, book and description. The mock writes no element, so the copy's other keys stay as they were copied. */
export function copiedGenerated(real: HomebrewGeneratedView, own: { id: string; name: string; source: string; description: string }): HomebrewGeneratedView {
  const parsed: unknown = JSON.parse(real.json);
  if (Array.isArray(parsed) || typeof parsed !== "object" || parsed === null) return real;
  const element: Record<string, unknown> = { ...(parsed as Record<string, unknown>), id: own.id, name: own.name, source: own.source };
  if (own.description) element.description = own.description;
  else delete element.description;
  const values: Record<string, string> = { id: own.id, name: own.name, source: own.source };
  const top = real.tree[0];
  const tree: HomebrewDataNode[] = top ? [{
    ...top, key: own.name,
    children: (top.children ?? []).flatMap((n): HomebrewDataNode[] => {
      if (n.key in values) return [{ ...n, text: values[n.key] }];
      if (n.key === "description") return own.description ? [{ ...n, text: own.description }] : [];
      return [n];
    }),
  }] : [];
  return { json: `${JSON.stringify(element, null, 2)}\n`, tree };
}

/** True when `id` stands in `v` as a whole token, anywhere (the backend's `pack::mentions`). */
export function mentions(v: unknown, id: string): boolean {
  if (typeof v === "string") return v.split(/[^A-Za-z0-9_]+/).includes(id);
  if (Array.isArray(v)) return v.some((x) => mentions(x, id));
  if (v && typeof v === "object") return Object.entries(v).some(([k, x]) => mentions(k, id) || mentions(x, id));
  return false;
}

/** How an own form uses `id`: a block gives it, the prerequisite needs it, anything else lists it. */
export function formUse(form: HomebrewForm, id: string): HomebrewUseRelation | null {
  if (form.blocks.some((b) => mentions(b.fields, id))) return "grants";
  if (mentions(form.fields.requirements, id)) return "requires";
  const rest = Object.fromEntries(Object.entries(form.fields).filter(([k]) => !["name", "description", "requirements"].includes(k)));
  return mentions(rest, id) || form.children.some((c) => formUse(c, id)) ? "lists" : null;
}

/** The sample content tags spells by class id in lower case ("wizard"); a list is named by its class ("Wizard"). */
export const hasTag = (tags: readonly string[], tag: string): boolean => tags.some((t) => t.toLowerCase() === tag.toLowerCase());

/** The loaded classes whose list a spell with `tags` is on (the backend's `spellcasting.list` match). */
export function listingClasses(loaded: readonly ElementSummary[], tags: readonly string[]): HomebrewElementRef[] {
  return loaded.filter((e) => e.type === "class" && (hasTag(tags, e.name) || hasTag(tags, e.id)))
    .map((e) => ({ id: e.id, name: e.name, type: "Class", packId: null, relation: "lists" as const }));
}

/** One SPELL LISTS row of a loaded spell: its picker row, on the list by its own tags or the pack's append. */
export function loadedListRow(e: ElementSummary, tag: string, appended: readonly string[]): HomebrewSpellListRow {
  const own = hasTag(e.tags ?? [], tag);
  return { ...pickerRow(e), type: "Spell", onList: own || appended.includes(tag), removable: !own && appended.includes(tag) };
}
