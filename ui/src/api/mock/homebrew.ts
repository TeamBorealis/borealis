import type {
  ApiError, BookRuleset, ContentPack, ElementDetail, ElementType, HomebrewDeleteResult, HomebrewDraftSummary, HomebrewDraftView, HomebrewElementSummary, HomebrewFieldSpec,
  HomebrewForm, HomebrewKindDescriptor, HomebrewKindId, HomebrewPackPatch, HomebrewPackRequest, HomebrewPickerRow, HomebrewProblem, HomebrewSaveResult, Ruleset,
} from "../types";
import { apiError } from "./errors";
import { descriptionHtml, formProblems } from "./homebrew-check";
import { MOCK_BLOCK_CATALOG } from "./homebrew-catalog";

/** The backend's answer for a creator command whose work item has not landed (contract: `details.notImplemented`). */
export function notImplemented(command: string): ApiError {
  return { code: "requirement_failed", message: `The homebrew creator can't do this yet (${command} is not implemented).`, details: { notImplemented: command } };
}

const GENERAL = { id: "general", label: "GENERAL" };
const DESCRIPTION = { id: "description", label: "DESCRIPTION" };
const RULES = { id: "rules", label: "RULES" };

const field = (key: string, label: string, control: HomebrewFieldSpec["control"], tab: string, bindings: string[], required = false): HomebrewFieldSpec =>
  ({ key, label, control, tab, bindings, required });

const choices = (...labels: string[]) => labels.map((label) => ({ value: label, label }));

/** The parts every form shares (spec 3.2), as `borealis-authoring::form::kinds` describes them (WB-04). */
const SHARED: HomebrewFieldSpec[] = [
  field("name", "Name", "text", "general", ["name"], true),
  { ...field("edition", "Edition", "combo", "general", ["ruleset"]), choices: [{ value: "", label: "Pack default" }, { value: "dnd2014", label: "2014" }, { value: "dnd2024", label: "2024" }] },
  { ...field("source", "Source", "text", "general", ["source"]), expert: true, readOnly: true },
  field("tags", "Tags", "tags", "general", ["supports"]),
  field("summary", "Summary line", "text", "general", ["setter:short"]),
  field("requirements", "Prerequisite", "condition", "general", ["requirements"]),
  field("prerequisiteText", "Prerequisite as shown", "text", "general", ["prerequisite"]),
  field("allowDuplicate", "Can be taken more than once", "check", "general", ["setter:allow duplicate"]),
  field("keywords", "Keywords", "chips", "general", ["setter:keywords"]),
  field("rules", "Rules", "blocks", "rules", ["rules"]),
  field("description", "Description", "richText", "description", ["description"]),
  field("sheet", "On the character sheet", "sheet", "description", ["sheet"]),
  field("compendium", "Show in Compendium", "check", "description", ["compendium.display"]),
  { ...field("mapsTo", "2014/2024 counterpart", "mapsTo", "description", ["maps_to"]), expert: true },
];

const kind = (id: string, label: string, pluralLabel: string, elementType: string, description: string, extra: HomebrewFieldSpec[] = []): HomebrewKindDescriptor => ({
  id, label, pluralLabel, description, group: "main", elementTypes: [elementType], parentKinds: [], editions: ["dnd2014", "dnd2024"],
  tabs: [GENERAL, RULES, DESCRIPTION], fields: [...SHARED, ...extra],
});

/** Sample forms for the web mock; the real descriptors come from `borealis-authoring` (WB-04). */
const KINDS: HomebrewKindDescriptor[] = [
  kind("spell", "Spell", "Spells", "Spell", "A spell for class spell lists.", [
    { ...field("level", "Level", "number", "general", ["setter:level", "data.level"], true), min: 0, max: 9 },
    { ...field("school", "School", "combo", "general", ["setter:school", "data.school"], true), choices: choices("", "Abjuration", "Conjuration", "Divination", "Enchantment", "Evocation", "Illusion", "Necromancy", "Transmutation") },
  ]),
  kind("feat", "Feat", "Feats", "Feat", "A feat characters can take.", [
    { ...field("category", "Category", "combo", "general", ["setter:category", "data.category"]), choices: choices("", "Origin", "General", "Fighting Style", "Epic Boon"), allowCustom: true },
  ]),
  kind("language", "Language", "Languages", "Language", "A language characters can learn."),
  {
    ...kind("other", "Other element", "Other elements", "Companion", "Any other kind of element, with the parts every form shares (expert).", [
      { ...field("type", "Type", "combo", "general", ["type"], true), choices: choices("Companion", "Familiar", "Property"), allowCustom: true },
      { ...field("setters", "Other properties", "setters", "general", ["setters"]), expert: true },
    ]),
    group: "expert",
    elementTypes: [],
  },
];

const TYPE_TOKENS: Record<string, string> = { Spell: "SPELL", Feat: "FEAT", Language: "LANGUAGE", Companion: "COMPANION" };
const PREVIEW_TYPES: Record<string, ElementType> = { spell: "spell", feat: "feat", language: "language" };
const slug = (text: string): string => text.toUpperCase().replace(/[^A-Z0-9]+/g, "_").replace(/^_+|_+$/g, "") || "UNNAMED";
const now = (): string => new Date().toISOString();

interface OwnPack {
  pack: ContentPack;
  code: string;
  elements: Map<string, { summary: HomebrewElementSummary; form: HomebrewForm }>;
}

const rulesetsOf = (edition: BookRuleset): Ruleset[] => (edition === "any" ? ["dnd2014", "dnd2024"] : [edition]);

/** In-memory homebrew creator for the web mock: packs, elements and drafts; the rest answers `notImplemented`. */
export class MockHomebrew {
  private packs = new Map<string, OwnPack>();
  private drafts = new Map<string, HomebrewDraftView>();
  /** Drafts that received an edit: only those are "unsaved changes" (spec 10.1: a draft is written by every update). */
  private touched = new Set<string>();
  private nextDraft = 1;

  constructor(private putPack: (pack: ContentPack) => ContentPack) {}

  kinds(): HomebrewKindDescriptor[] { return KINDS; }

  createPack(req: HomebrewPackRequest): ContentPack {
    const name = req.name.trim();
    if (!name) throw apiError("invalid_argument", "Give the pack a name.");
    if (!/^[A-Z0-9]{2,8}$/.test(req.code)) throw apiError("invalid_argument", "The pack code is 2 to 8 capital letters or digits.");
    if ([...this.packs.values()].some((p) => p.code === req.code)) throw apiError("invalid_argument", `Another pack already uses the code ${req.code}.`);
    const id = `homebrew-${req.code.toLowerCase()}`;
    const pack: ContentPack = {
      id, name, version: "1.0.0", kind: "homebrew", rulesets: rulesetsOf(req.edition), elementCount: 0, enabled: true,
      license: req.license ?? "none", attribution: req.author ?? "", sourceIds: [`source-${id}`],
    };
    this.packs.set(id, { pack, code: req.code, elements: new Map() });
    return this.putPack(pack);
  }

  updatePack(packId: string, patch: HomebrewPackPatch): ContentPack {
    const own = this.own(packId);
    const pack: ContentPack = {
      ...own.pack, name: patch.name?.trim() || own.pack.name, rulesets: patch.edition ? rulesetsOf(patch.edition) : own.pack.rulesets,
      license: patch.license ?? own.pack.license, attribution: patch.author ?? own.pack.attribution,
    };
    this.packs.set(packId, { ...own, pack });
    return this.putPack(pack);
  }

  listElements(packId: string): HomebrewElementSummary[] {
    const own = this.own(packId);
    const drafted = new Set([...this.drafts.values()].filter((d) => this.touched.has(d.draftId)).map((d) => d.elementId));
    return [...own.elements.values()].map(({ summary }) => ({ ...summary, hasDraft: drafted.has(summary.id) }));
  }

  newDraft(packId: string, kindId: HomebrewKindId, copyFrom?: string): HomebrewDraftView {
    this.own(packId);
    if (copyFrom) throw notImplemented("homebrew_new_draft (copy)");
    if (!KINDS.some((k) => k.id === kindId)) throw apiError("invalid_argument", `Unknown kind ${kindId}.`);
    return this.store(packId, null, { kind: kindId, fields: { name: "" }, blocks: [], children: [], preserved: [] });
  }

  openDraft(packId: string, elementId: string): HomebrewDraftView {
    const existing = [...this.drafts.values()].find((d) => d.elementId === elementId);
    if (existing) return existing;
    const saved = this.own(packId).elements.get(elementId);
    if (!saved) throw apiError("not_found", "That element isn't in this pack any more.");
    return this.store(packId, elementId, saved.form);
  }

  updateDraft(draftId: string, form: HomebrewForm): HomebrewDraftView {
    const draft = this.draft(draftId);
    this.touched.add(draftId);
    return this.store(draft.packId, draft.elementId, form, draftId);
  }

  checkDraft(draftId: string): HomebrewProblem[] { return this.draft(draftId).problems; }

  saveDraft(draftId: string): HomebrewSaveResult {
    const draft = this.draft(draftId);
    if (draft.problems.some((p) => p.severity === "error")) {
      throw { ...apiError("invalid_argument", "Fix the problems marked in red before saving."), details: draft.problems };
    }
    const own = this.own(draft.packId);
    const k = KINDS.find((x) => x.id === draft.form.kind)!;
    const type = k.elementTypes[0] ?? (String(draft.form.fields.type ?? "").trim() || k.label);
    const name = String(draft.form.fields.name).trim();
    const id = draft.elementId ?? this.freeId(own, `ID_${own.code}_${TYPE_TOKENS[type] ?? slug(type)}_${slug(name)}`);
    const summary: HomebrewElementSummary = { id, name, kind: k.id, elementType: type, parentId: null, edition: null, problems: { errors: 0, warnings: 0 }, hasDraft: false };
    const elements = new Map(own.elements).set(id, { summary, form: draft.form });
    const pack = { ...own.pack, elementCount: elements.size, version: bumpPatch(own.pack.version) };
    this.packs.set(own.pack.id, { ...own, pack, elements });
    this.drafts.delete(draftId);
    this.touched.delete(draftId);
    return { pack: this.putPack(pack), elementIds: [id] };
  }

  discardDraft(draftId: string): void {
    this.draft(draftId);
    this.drafts.delete(draftId);
    this.touched.delete(draftId);
  }

  listDrafts(): HomebrewDraftSummary[] {
    return [...this.drafts.values()].filter((d) => this.touched.has(d.draftId)).map((d) => ({ draftId: d.draftId, packId: d.packId, elementId: d.elementId, kind: d.form.kind, name: String(d.form.fields.name ?? ""), updatedAt: d.updatedAt }));
  }

  deleteElements(packId: string, elementIds: string[]): HomebrewDeleteResult {
    const own = this.own(packId);
    const elements = new Map([...own.elements].filter(([id]) => !elementIds.includes(id)));
    const pack = { ...own.pack, elementCount: elements.size, version: bumpPatch(own.pack.version) };
    this.packs.set(packId, { ...own, pack, elements });
    return { deleted: true, usedBy: [], pack: this.putPack(pack) };
  }

  duplicateElement(packId: string, elementId: string): HomebrewDraftView {
    const saved = this.own(packId).elements.get(elementId);
    if (!saved) throw apiError("not_found", "That element isn't in this pack any more.");
    const form = { ...saved.form, fields: { ...saved.form.fields, name: `${String(saved.form.fields.name)} (copy)` } };
    return this.store(packId, null, form);
  }

  /** Picker rows of every own pack's saved elements, and of the open drafts of `draftId`'s pack (spec 5.1: "plus the current
   * pack's unsaved drafts"). */
  pickerRows(draftId?: string): HomebrewPickerRow[] {
    const draftPack = draftId ? this.drafts.get(draftId)?.packId : undefined;
    const saved = [...this.packs.values()].flatMap((own) => [...own.elements.values()].map(({ summary, form }): HomebrewPickerRow => ({
      id: summary.id, name: summary.name, type: summary.elementType, summary: String(form.fields.summary ?? ""), prerequisites: null,
      sourceName: own.pack.name, edition: summary.edition, draft: false,
    })));
    const drafts = [...this.drafts.values()].filter((d) => d.packId === draftPack && d.elementId === null && d.draftId !== draftId && String(d.form.fields.name ?? "").trim())
      .map((d): HomebrewPickerRow => ({ id: `draft:${d.draftId}`, name: String(d.form.fields.name).trim(), type: KINDS.find((k) => k.id === d.form.kind)?.elementTypes[0] ?? "", summary: "", prerequisites: null, sourceName: this.packs.get(d.packId)?.pack.name ?? "", edition: null, draft: true }));
    return [...saved, ...drafts];
  }

  private own(packId: string): OwnPack {
    const own = this.packs.get(packId);
    if (!own) throw apiError("not_found", "That homebrew pack isn't installed.");
    return own;
  }

  private draft(draftId: string): HomebrewDraftView {
    const d = this.drafts.get(draftId);
    if (!d) throw apiError("not_found", "That draft was closed.");
    return d;
  }

  /** The live description pane: the name, the description text and the kind's facts, as the builder shows an element. */
  private preview(packId: string, id: string, form: HomebrewForm): ElementDetail {
    const own = this.own(packId);
    const k = KINDS.find((x) => x.id === form.kind);
    const text = (key: string) => (typeof form.fields[key] === "string" ? String(form.fields[key]).trim() : "");
    const level = form.fields.level;
    const facts = [
      ...(typeof level === "number" ? [{ label: "Level", value: level === 0 ? "Cantrip" : String(level) }] : []),
      ...["school", "category"].filter((key) => text(key)).map((key) => ({ label: key === "school" ? "School" : "Category", value: text(key) })),
    ];
    const description = descriptionHtml(form.fields.description);
    return {
      id, name: text("name") || "Unnamed", type: PREVIEW_TYPES[form.kind] ?? "option", source: own.pack.sourceIds[0] ?? own.pack.id,
      sourceName: own.pack.name, shortDescription: "", prerequisites: null, rulesets: own.pack.rulesets,
      descriptionHtml: description, facts: k ? facts : [], grants: [],
    };
  }

  private freeId(own: OwnPack, base: string): string {
    if (!own.elements.has(base)) return base;
    for (let n = 2; ; n += 1) if (!own.elements.has(`${base}_${n}`)) return `${base}_${n}`;
  }

  private store(packId: string, elementId: string | null, form: HomebrewForm, draftId = `draft-${this.nextDraft++}`): HomebrewDraftView {
    const problems = formProblems(form, MOCK_BLOCK_CATALOG, elementId);
    const view: HomebrewDraftView = {
      draftId, packId, elementId, form, preview: this.preview(packId, elementId ?? draftId, form), problems, updatedAt: now(), baseChanged: false,
      generated: { json: JSON.stringify({ name: form.fields.name ?? "" }, null, 2), tree: [] },
    };
    this.drafts.set(draftId, view);
    return view;
  }
}


function bumpPatch(version: string): string {
  const [major = "1", minor = "0", patch = "0"] = version.split(".");
  return `${major}.${minor}.${Number(patch) + 1}`;
}
