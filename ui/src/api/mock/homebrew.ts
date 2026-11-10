import type {
  ApiError, BookRuleset, ContentPack, ElementDetail, ElementType, HomebrewDeleteResult, HomebrewDraftSummary, HomebrewDraftView, HomebrewElementSummary, HomebrewFieldSpec,
  HomebrewForm, HomebrewKindDescriptor, HomebrewKindId, HomebrewPackPatch, HomebrewPackRequest, HomebrewPickerRow, HomebrewProblem, HomebrewSaveResult, Ruleset,
} from "../types";
import { apiError } from "./errors";
import { descriptionHtml, formProblems } from "./homebrew-check";
import { addChild, newChildKey, newClassForm, newForm } from "./homebrew-class";
import { featPrerequisite } from "./homebrew-feat";
import { MOCK_BLOCK_CATALOG } from "./homebrew-catalog";
import { CORE_PICKER_ROWS } from "./homebrew-core-rows";
import { MOCK_ITEM_KINDS } from "./homebrew-item-kinds";
import { MOCK_KIND_DESCRIPTORS } from "./homebrew-kinds";
import type { TrialDraft } from "./homebrew-trial";

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

/** Kinds whose own form has landed in `borealis-authoring`: the mock serves their real descriptors (generated). */
const REAL_KINDS = ["spell", "feat", "featFeature", "class", "classFeature"];
const realKinds = (): HomebrewKindDescriptor[] => REAL_KINDS.map((id) => {
  const k = MOCK_KIND_DESCRIPTORS.find((d) => d.id === id);
  if (!k) throw new Error(`homebrew-kinds.ts has no ${id} form`);
  return k;
});

/** The forms of the web mock: the real ones (WB-11 spell, WB-13 feat), then samples for kinds whose form is still to come. */
const KINDS: HomebrewKindDescriptor[] = [
  ...realKinds(),
  kind("language", "Language", "Languages", "Language", "A language characters can learn."),
  // The item forms (WB-12), as borealis-authoring describes them.
  ...MOCK_ITEM_KINDS,
  {
    ...kind("other", "Other element", "Other elements", "Companion", "Any other kind of element, with the parts every form shares (expert).", [
      { ...field("type", "Type", "combo", "general", ["type"], true), choices: choices("Companion", "Familiar", "Property"), allowCustom: true },
      { ...field("setters", "Other properties", "setters", "general", ["setters"]), expert: true },
    ]),
    group: "expert",
    elementTypes: [],
  },
];

const TYPE_TOKENS: Record<string, string> = {
  Spell: "SPELL", Feat: "FEAT", "Feat Feature": "FEAT_FEATURE", Language: "LANGUAGE", Companion: "COMPANION", Class: "CLASS", "Class Feature": "CLASS_FEATURE",
};
const PREVIEW_TYPES: Record<string, ElementType> = {
  spell: "spell", feat: "feat", language: "language", class: "class", classFeature: "class-feature",
  item: "item", weapon: "weapon", armor: "armor", magicItem: "magic-item", itemPack: "item", equipmentPackage: "option",
};
/** Item facts the description pane lists (WB-12): field key and label. */
const ITEM_FACTS: [string, string][] = [["rarity", "Rarity"], ["damage", "Damage"], ["cost", "Cost"], ["weight", "Weight (lb.)"]];
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

  newDraft(packId: string, kindId: HomebrewKindId, copyFrom?: string, parentId?: string): HomebrewDraftView {
    const own = this.own(packId);
    if (copyFrom) throw notImplemented("homebrew_new_draft (copy)");
    const k = KINDS.find((x) => x.id === kindId);
    if (!k) throw apiError("invalid_argument", `Unknown kind ${kindId}.`);
    if (parentId) return this.newChild(packId, k, parentId);
    // A class is of one edition and starts with its generated features (WB-16); every other kind is `KindDef::new_form`.
    const form = k.id === "class"
      ? newClassForm(KINDS, own.pack.rulesets.length === 1 && own.pack.rulesets[0] === "dnd2014" ? "dnd2014" : "dnd2024")
      : { ...newForm(k, "e"), key: undefined };
    return this.store(packId, null, form);
  }

  /** `homebrew_new_draft {parentId}` (WB-16): a child inside its parent's draft (or a saved element's), given by the
   * parent's field for it; the answer is the parent's draft. */
  private newChild(packId: string, kind: HomebrewKindDescriptor, parentId: string): HomebrewDraftView {
    const draft = this.drafts.get(parentId) ?? this.openDraft(packId, parentId);
    const next = addChild(draft.form, newForm(kind, newChildKey(draft.form)), KINDS);
    if (!next) throw apiError("invalid_argument", `A ${kind.label.toLowerCase()} can't be made inside this element.`);
    this.touched.add(draft.draftId);
    return this.store(packId, draft.elementId, next, draft.draftId);
  }

  openDraft(packId: string, elementId?: string, draftId?: string): HomebrewDraftView {
    if (draftId) {
      const draft = this.draft(draftId);
      if (draft.packId !== packId) throw apiError("not_found", "That draft belongs to another pack.");
      return draft;
    }
    if (!elementId) throw apiError("invalid_argument", "Say which element to open.");
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

  /** The engine checks (test character, level sweep, balance hints) need the engine: the mock has none to report. */
  checkDraft(draftId: string): HomebrewProblem[] { this.draft(draftId); return []; }

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
      id: summary.id, name: summary.name, type: summary.elementType, summary: String(form.fields.summary ?? ""), prerequisites: featPrerequisite(form),
      sourceName: own.pack.name, edition: summary.edition, draft: false, ...spellRow(form),
    })));
    const drafts = [...this.drafts.values()].filter((d) => d.packId === draftPack && d.elementId === null && d.draftId !== draftId && String(d.form.fields.name ?? "").trim())
      .map((d): HomebrewPickerRow => ({ id: `draft:${d.draftId}`, name: String(d.form.fields.name).trim(), type: KINDS.find((k) => k.id === d.form.kind)?.elementTypes[0] ?? "", summary: "", prerequisites: null, sourceName: this.packs.get(d.packId)?.pack.name ?? "", edition: null, draft: true }));
    return [...saved, ...drafts, ...CORE_PICKER_ROWS];
  }

  /** A draft for Try it (spec 9): its last view, its kind and its pack's editions. */
  trialDraft(draftId: string): TrialDraft {
    const view = this.draft(draftId);
    const kind = KINDS.find((k) => k.id === view.form.kind);
    if (!kind) throw apiError("invalid_argument", `Unknown kind ${view.form.kind}.`);
    return { view, kind, rulesets: this.own(view.packId).pack.rulesets };
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
    const level = spellLevel(form);
    const facts = [
      ...(level !== undefined ? [{ label: "Level", value: level === 0 ? "Cantrip" : String(level) }] : []),
      ...["school", "category"].filter((key) => text(key)).map((key) => ({ label: key === "school" ? "School" : "Category", value: text(key) })),
      ...ITEM_FACTS.filter(([key]) => text(key)).map(([key, label]) => ({ label, value: text(key) })),
      ...(typeof form.fields.enhancement === "number" && form.fields.enhancement > 0 ? [{ label: "Bonus", value: `+${form.fields.enhancement}` }] : []),
      ...(form.kind === "spell" ? SPELL_FACTS.filter(([key]) => text(key)).map(([key, label]) => ({ label, value: text(key) })) : []),
    ];
    const description = descriptionHtml(form.fields.description);
    return {
      id, name: text("name") || "Unnamed", type: PREVIEW_TYPES[form.kind] ?? "option", source: own.pack.sourceIds[0] ?? own.pack.id,
      sourceName: own.pack.name, shortDescription: "", prerequisites: featPrerequisite(form), rulesets: own.pack.rulesets,
      descriptionHtml: description, facts: k ? facts : [], grants: [],
    };
  }

  private freeId(own: OwnPack, base: string): string {
    if (!own.elements.has(base)) return base;
    for (let n = 2; ; n += 1) if (!own.elements.has(`${base}_${n}`)) return `${base}_${n}`;
  }

  private store(packId: string, elementId: string | null, form: HomebrewForm, draftId = `draft-${this.nextDraft++}`): HomebrewDraftView {
    const problems = [
      ...formProblems(form, MOCK_BLOCK_CATALOG, elementId, KINDS.find((k) => k.id === form.kind)),
      // a child's problems, at its path (`children.0.fields.name`)
      ...form.children.flatMap((c, i) => formProblems(c, MOCK_BLOCK_CATALOG, null, KINDS.find((k) => k.id === c.kind))
        .map((p) => ({ ...p, path: `children.${i}.${p.path ?? ""}`.replace(/\.$/, "") }))),
    ];
    const view: HomebrewDraftView = {
      draftId, packId, elementId, form, preview: this.preview(packId, elementId ?? draftId, form), problems, updatedAt: now(), baseChanged: false,
      generated: { json: JSON.stringify({ name: form.fields.name ?? "" }, null, 2), tree: [] },
    };
    this.drafts.set(draftId, view);
    return view;
  }
}


/** A spell form's level ("0"-"9", as the level combo stores it). */
function spellLevel(form: HomebrewForm): number | undefined {
  const n = Number(form.fields.level);
  return form.kind === "spell" && form.fields.level !== "" && Number.isInteger(n) && n >= 0 && n <= 9 ? n : undefined;
}

/** The spell facts the live description pane lists after level and school. */
const SPELL_FACTS: readonly (readonly [string, string])[] = [["time", "Casting Time"], ["range", "Range"], ["duration", "Duration"]];

/** A saved spell's picker columns (the backend's `homebrew_refs` spell facts): level, school, V/S/M, ritual, concentration. */
function spellRow(form: HomebrewForm): Pick<HomebrewPickerRow, "level" | "school" | "spell"> {
  const level = spellLevel(form);
  if (level === undefined) return {};
  const f = form.fields;
  const str = (v: unknown): string => (typeof v === "string" ? v.trim() : "");
  return {
    level, school: str(f.school),
    spell: {
      castingTime: str(f.time), range: str(f.range), duration: str(f.duration), verbal: f.verbal === true, somatic: f.somatic === true,
      material: f.material === true, materialText: str(f.materialText), concentration: f.concentration === true, ritual: f.ritual === true,
    },
  };
}

function bumpPatch(version: string): string {
  const [major = "1", minor = "0", patch = "0"] = version.split(".");
  return `${major}.${minor}.${Number(patch) + 1}`;
}
