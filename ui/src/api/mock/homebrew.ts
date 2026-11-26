import type {
  ApiError, BookRuleset, ContentPack, ElementDetail, ElementFact, ElementSummary, ElementType, HomebrewBulkPatch, HomebrewBulkResult, HomebrewCharacterRef, HomebrewDeleteResult, HomebrewDraftSummary,
  HomebrewDraftView, HomebrewElementRef, HomebrewElementSummary, HomebrewForm, HomebrewKindDescriptor, HomebrewKindId, HomebrewPackPatch, HomebrewPackRequest,
  HomebrewPickerRow, HomebrewProblem, HomebrewSaveResult, HomebrewSpellListRow, HomebrewSpellListView, Ruleset,
  HomebrewExportResult, HomebrewOverrideInfo, ImportFileInput, ImportPackResult, ImportPreview, TypeCount, ElementChangedBy,
} from "../types";
import { copiedGenerated, formUse, generatedOf, hasTag, listingClasses, loadedListRow, tagsOf, textOf, withTags } from "./homebrew-expert";
import { apiError } from "./errors";
import { descriptionHtml, formProblems, pickerRow, spellListProblems } from "./homebrew-check";
import { addChild, addChildIn, newChildKey, newClassForm, newForm } from "./homebrew-class";
import { backgroundProblems, FEAT_PARAMETER_PICKER, featParameterWords } from "./homebrew-background";
import { creatureFacts, creatureHtml, creatureProblems } from "./homebrew-creature";
import { featPrerequisite } from "./homebrew-feat";
import { refreshSubclass, subclassProblems } from "./homebrew-subclass";
import { MOCK_BLOCK_CATALOG } from "./homebrew-catalog";
import { CORE_PICKER_ROWS } from "./homebrew-core-rows";
import { MOCK_COPY_SOURCES, type MockCopySource } from "./homebrew-copies";
import { MOCK_KIND_DESCRIPTORS } from "./homebrew-kinds";
import { contentSkillNames, SKILL_NAME_PICKER, skillKey } from "./homebrew-skills";
import type { TrialDraft } from "./homebrew-trial";
import { copyCode, copyOf, type MockPackData, packFile, packFileName, readPackFile } from "./homebrew-transfer";
import { toBase64 } from "./pack-file";

/** The backend's answer for a creator command whose work item has not landed (contract: `details.notImplemented`). */
export function notImplemented(command: string): ApiError {
  return { code: "requirement_failed", message: `The homebrew creator can't do this yet (${command} is not implemented).`, details: { notImplemented: command } };
}

/** The forms of the web mock: every kind `borealis-authoring` defines, as the backend's `homebrew_kinds` answers them
 * (generated into `homebrew-kinds.ts`), in its order. */
const KINDS: readonly HomebrewKindDescriptor[] = MOCK_KIND_DESCRIPTORS;
/** The item forms (spec 4.9): their saved elements' categories are the Category suggestions. */
const ITEM_KIND_IDS: readonly HomebrewKindId[] = ["item", "weapon", "armor", "magicItem", "itemPack", "equipmentPackage"];

/** The pseudo picker types of the generic form (`borealis_authoring::form::{ELEMENT_TYPE_PICKER, SETTER_NAME_PICKER}`). */
export const ELEMENT_TYPE_PICKER = "Element Type";
/** SPELL LISTS' expanded-list choices (the backend's `homebrew_refs::EXPANDED_LIST_TYPE`). */
export const EXPANDED_LIST_PICKER = "Expanded Spell List";
export const SETTER_NAME_PICKER = "Setter Name";
/** The item forms' Category suggestions and the Tags field's (`borealis_authoring::form::{ITEM_CATEGORY_PICKER, TAG_PICKER}`). */
export const ITEM_CATEGORY_PICKER = "Item Category";
export const TAG_PICKER = "Tag";

const TYPE_TOKENS: Record<string, string> = {
  Spell: "SPELL", Feat: "FEAT", "Feat Feature": "FEAT_FEATURE", Species: "SPECIES", Race: "RACE", "Sub Race": "SUBRACE", "Race Variant": "RACE_VARIANT",
  "Racial Trait": "TRAIT", "Species Trait Option": "TRAIT_OPTION", Language: "LANGUAGE", Companion: "COMPANION", Class: "CLASS",
  "Class Feature": "CLASS_FEATURE", Archetype: "SUBCLASS", "Archetype Feature": "SUBCLASS_FEATURE", Option: "OPTION",
  Proficiency: "PROFICIENCY", Condition: "CONDITION", "Damage Type": "DAMAGE_TYPE", Skill: "SKILL", Deity: "DEITY", Vision: "VISION", Size: "SIZE",
  Alignment: "ALIGNMENT", "Weapon Property": "WEAPON_PROPERTY", "Weapon Group": "WEAPON_GROUP", "Weapon Category": "WEAPON_CATEGORY",
  "Armor Group": "ARMOR_GROUP", "Mastery Property": "MASTERY", "Spellcasting Focus Group": "FOCUS_GROUP", Dragonmark: "DRAGONMARK",
  "Rules Glossary Entry": "RULE", Familiar: "FAMILIAR", Background: "BACKGROUND", "Background Feature": "BACKGROUND_FEATURE",
  "Background Variant": "BACKGROUND_VARIANT", "Background Characteristics": "BACKGROUND_CHARACTERISTICS", "Equipment Package": "EQUIPMENT_PACKAGE",
  Monster: "CREATURE",
};
const PREVIEW_TYPES: Record<string, ElementType> = {
  spell: "spell", feat: "feat", language: "language", species: "race", race: "race", subRace: "subrace", raceVariant: "subrace", racialTrait: "racial-trait",
  speciesTraitOption: "racial-trait", class: "class", classFeature: "class-feature", subclass: "archetype", subclassFeature: "class-feature",
  classOption: "option", proficiency: "proficiency", skill: "skill", deity: "deity", other: "companion",
  background: "background", backgroundFeature: "background", backgroundVariant: "background", backgroundCharacteristics: "background",
  item: "item", weapon: "weapon", armor: "armor", magicItem: "magic-item", itemPack: "item", equipmentPackage: "option", creature: "monster",
};
/** A picked reference's name (`{id, name, type}`), or the names of a list of them. */
const refName = (v: unknown): string => (typeof v === "object" && v !== null && !Array.isArray(v) ? String((v as { name?: unknown }).name ?? "").trim() : "");
const refNames = (v: unknown): string[] => (Array.isArray(v) ? v.map(refName).filter(Boolean) : []);

/** An item draft's facts as the backend's compendium entry lists them (`compendium::facts`, WB-12, P2b): rarity, the
 * weapon category, the cost with its coin, the damage with its type, the properties, the mastery and the weight. */
function itemFacts(form: HomebrewForm): ElementFact[] {
  const f = form.fields;
  const text = (key: string) => (typeof f[key] === "string" ? String(f[key]).trim() : "");
  const versatile = text("versatile");
  const properties = refNames(f.properties).filter((p) => !versatile || p.toLowerCase() !== "versatile");
  if (versatile) properties.push(`Versatile (${versatile})`);
  const cost = Number(text("cost"));
  const damage = text("damage");
  const damageType = refName(f.damageType).toLowerCase();
  const out: [string, string][] = [
    ["Rarity", text("rarity")],
    ["Weapon category", refName(f.weaponCategory)],
    ["Cost", cost > 0 ? `${text("cost")} ${text("currency") || "gp"}` : ""],
    ["Damage", damage ? `${damage}${damageType ? ` ${damageType}` : ""}` : ""],
    ["Properties", properties.join(", ")],
    ["Mastery", refNames(f.mastery).join(", ")],
    ["Weight", text("weight") ? `${text("weight")} lb.` : ""],
  ];
  return out.filter(([, v]) => v).map(([label, value]) => ({ label, value }));
}
const slug = (text: string): string => text.toUpperCase().replace(/[^A-Z0-9]+/g, "_").replace(/^_+|_+$/g, "") || "UNNAMED";
const now = (): string => new Date().toISOString();

interface OwnPack {
  pack: ContentPack;
  code: string;
  elements: Map<string, { summary: HomebrewElementSummary; form: HomebrewForm }>;
  /** "Add to this list" appends (HC5): a loaded spell's id and the list tags the pack adds to it. */
  appends: Map<string, string[]>;
}

/** An undoable change (spec 10.2, 12.5): the packs it wrote as they were, and the version each change wrote. */
interface UndoRecord { packs: { before: OwnPack; versionAfter: string }[] }

/** As the backend lists it (`content_packs::kind_of`, CS1/HC8): a pack holding text copied from the user's imports is
 * not redistributable. */
function marked(pack: ContentPack, elements: OwnPack["elements"]): ContentPack {
  const { redistributable: _, ...rest } = pack;
  return [...elements.values()].some((e) => e.summary.copiedText) ? { ...rest, redistributable: false } : rest;
}

const rulesetsOf = (edition: BookRuleset): Ruleset[] => (edition === "any" ? ["dnd2014", "dnd2024"] : [edition]);

/** In-memory homebrew creator for the web mock: packs, elements and drafts; the rest answers `notImplemented`. */
export class MockHomebrew {
  private packs = new Map<string, OwnPack>();
  private drafts = new Map<string, HomebrewDraftView>();
  /** Drafts that received an edit: only those are "unsaved changes" (spec 10.1: a draft is written by every update). */
  private touched = new Set<string>();
  private nextDraft = 1;
  /** A deep class copy's subclasses, saved with it (spec 11). */
  private companions = new Map<string, HomebrewForm[]>();
  /** Drafts copied from loaded sample content that is not an SRD copy source: they stand for text copied from the user's
   * own imports (CS1, HC8), so their saved elements are `copiedText`. */
  private copiedText = new Set<string>();
  /** Drafts copied from a real copy source (`homebrew-copies.ts`): their DATA view is the backend's copy. */
  private copySources = new Map<string, MockCopySource>();
  /** HC20: override drafts and the original each one changes (spec 22). */
  private overrideOf = new Map<string, HomebrewOverrideInfo>();
  /** Pack files waiting in an import preview (spec 14.2). */
  private stagedFiles = new Map<string, { fileName: string; data: MockPackData }>();
  private nextPreview = 1;

  /** `spellLists`: the spell lists loaded classes declare (the spell form's list lints). */
  constructor(private putPack: (pack: ContentPack) => ContentPack, private spellLists: () => readonly string[] = () => [],
    private loadedName: (id: string) => string | null = () => null) {}

  /** An element's name by id, own packs first (the lookup the backend's prerequisite sentence reads). */
  private readonly nameOf = (id: string): string | null =>
    [...this.packs.values()].map((p) => p.elements.get(id)?.summary.name).find((n) => n !== undefined) ?? this.loadedName(id);

  kinds(): HomebrewKindDescriptor[] { return [...KINDS]; }

  /** A homebrew pack removed in Additional content (`remove_pack`): its elements and drafts go with it. */
  forgetPack(packId: string): void {
    if (!this.packs.delete(packId)) return;
    for (const [id, d] of this.drafts) {
      if (d.packId === packId) { this.drafts.delete(id); this.touched.delete(id); }
    }
  }

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
    this.packs.set(id, { pack, code: req.code, elements: new Map(), appends: new Map() });
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
    return [...own.elements.values()].map(({ summary, form }) => ({ ...summary, hasDraft: drafted.has(summary.id), tags: tagsOf(form), text: textOf(form) }));
  }

  newDraft(packId: string, kindId: HomebrewKindId, copyFrom?: string, parentId?: string, deepCopy?: boolean, loaded?: { name: string; type: ElementType }, parentKey?: string): HomebrewDraftView {
    const own = this.own(packId);
    if (copyFrom && !parentId) return this.copy(own, copyFrom, deepCopy === true, loaded);
    if (copyFrom) throw notImplemented("homebrew_new_draft (copy into a parent)");
    const k = KINDS.find((x) => x.id === kindId);
    if (!k) throw apiError("invalid_argument", `Unknown kind ${kindId}.`);
    if (parentId) return this.newChild(packId, k, parentId, parentKey);
    // A class, a background and a creature are of one edition (a pack of both makes 2024 ones); a class starts with its generated
    // features (WB-16); every other kind is `KindDef::new_form`.
    const edition: Ruleset = own.pack.rulesets.length === 1 && own.pack.rulesets[0] === "dnd2014" ? "dnd2014" : "dnd2024";
    const blank = { ...newForm(k, "e"), key: undefined };
    const form = k.id === "class" ? newClassForm(KINDS, edition) : k.id === "background" || k.id === "creature" ? { ...blank, fields: { ...blank.fields, edition } } : blank;
    return this.store(packId, null, form);
  }

  /** A copy (spec 11, WB-22): the real backend's copy of an SRD source (`homebrew-copies.ts`; a sample weapon or armor
   * takes the SRD one of its name, H10ac), an own element's form, or for other sample content a blank form of its kind
   * named "<Name> (copy)". A deep class copy saves its subclasses too. */
  private copy(own: OwnPack, id: string, deep: boolean, loaded?: { name: string; type: ElementType }): HomebrewDraftView {
    // an SRD copy source, or the one a loaded sample weapon or armor stands for (same name and type, H10ac)
    const srd = MOCK_COPY_SOURCES.find((c) => c.row.id === id);
    const sameItem = loaded && (loaded.type === "weapon" || loaded.type === "armor")
      ? MOCK_COPY_SOURCES.find((c) => c.row.name === loaded.name && c.row.type.toLowerCase() === loaded.type) : undefined;
    const source = srd ?? sameItem;
    const saved = [...this.packs.values()].map((p) => p.elements.get(id)).find((e) => e !== undefined);
    let form: HomebrewForm;
    let companions: HomebrewForm[] = [];
    if (source) {
      const chosen = deep && source.deep ? source.deep : { form: source.form, companions: [] };
      form = withSource(chosen.form, own.pack.name);
      companions = chosen.companions.map((c) => withSource(c, own.pack.name));
    } else if (saved) {
      form = { ...saved.form, fields: { ...saved.form.fields, name: `${String(saved.form.fields.name)} (copy)` } };
    } else if (loaded) {
      const k = KINDS.find((x) => PREVIEW_TYPES[x.id] === loaded.type) ?? KINDS.find((x) => x.id === "other")!;
      const blank = newForm(k, "e");
      form = { ...blank, fields: { ...blank.fields, name: `${loaded.name} (copy)`, source: own.pack.name } };
    } else {
      throw apiError("not_found", "That element isn't loaded any more.");
    }
    const draftId = `draft-${this.nextDraft++}`;
    if (source) this.copySources.set(draftId, source);
    const view = this.store(own.pack.id, null, form, draftId);
    if (!srd && !saved) this.copiedText.add(view.draftId);
    if (!companions.length) return view;
    this.companions.set(view.draftId, companions);
    const named = { ...view, ...this.savedWith(view.draftId) };
    this.drafts.set(view.draftId, named);
    return named;
  }

  /** `homebrew_new_draft {parentId}` (WB-16): a child inside its parent's draft (or a saved element's), given by the
   * parent's field for it; the answer is the parent's draft. */
  private newChild(packId: string, kind: HomebrewKindDescriptor, parentId: string, parentKey?: string): HomebrewDraftView {
    const draft = this.drafts.get(parentId) ?? this.openDraft(packId, parentId);
    const child = newForm(kind, newChildKey(draft.form));
    const next = parentKey ? addChildIn(draft.form, parentKey, child, KINDS) : addChild(draft.form, child, KINDS);
    if (!next) throw apiError("invalid_argument", `A ${kind.label.toLowerCase()} can't be made inside this element.`);
    this.touched.add(draft.draftId);
    return this.store(packId, draft.elementId, next, draft.draftId);
  }

  /** `homebrew_child_use`: the web mock has no characters on disk, so no saved child is used by one. */
  childUse(draftId: string, _childKey: string): HomebrewCharacterRef[] {
    this.draft(draftId);
    return [];
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
    const fresh = `draft-${this.nextDraft++}`;
    if (saved.summary.override) this.overrideOf.set(fresh, { ...saved.summary.override, changed: false });
    return this.store(packId, elementId, saved.form, fresh);
  }

  /** HC20 (spec 22.1): an override of a loaded element, every id kept: the SRD copy source's form under the original's
   * name, else a blank form of its kind; refused as the backend refuses. */
  newOverride(packId: string, elementId: string, loaded?: { name: string; type: ElementType; sourceName: string; school?: string }): HomebrewDraftView {
    const own = this.own(packId);
    if (own.elements.get(elementId)?.summary.override) return this.openDraft(packId, elementId);
    if (loaded?.type === "source") throw apiError("invalid_argument", "A book element can't be changed.");
    const home = [...this.packs.values()].find((p) => p.elements.has(elementId) && !p.elements.get(elementId)!.summary.override);
    if (home) throw apiError("invalid_argument", `That element is in your pack ${home.pack.name}. Open it there to change it.`);
    const by = this.changedBy(elementId);
    if (by) throw apiError("invalid_argument", `${by.packName} already changes this element. Change it there, or revert that change first.`);
    const srd = MOCK_COPY_SOURCES.find((c) => c.row.id === elementId);
    let form: HomebrewForm;
    if (srd) form = { ...srd.form, fields: { ...srd.form.fields, name: srd.row.name, ...("source" in srd.form.fields ? { source: srd.row.sourceName } : {}) } };
    else if (loaded) {
      const k = KINDS.find((x) => PREVIEW_TYPES[x.id] === loaded.type) ?? KINDS.find((x) => x.id === "other")!;
      const blank = newForm(k, "e");
      form = { ...blank, fields: { ...blank.fields, name: loaded.name, source: loaded.sourceName, ...(loaded.school && k.id === "spell" ? { school: loaded.school } : {}) } };
    } else throw apiError("not_found", "That element isn't loaded any more.");
    const draftId = `draft-${this.nextDraft++}`;
    this.overrideOf.set(draftId, { originalId: elementId, sourceName: srd?.row.sourceName ?? loaded?.sourceName ?? "", srd: !!srd, changed: false, missing: false });
    return this.store(packId, elementId, form, draftId);
  }

  /** HC20 (spec 22.6): Revert removes the override; undoable like a delete. */
  revertOverride(packId: string, elementId: string): HomebrewDeleteResult {
    if (!this.own(packId).elements.get(elementId)?.summary.override) throw apiError("not_found", "This pack doesn't change that element any more.");
    return this.deleteElements(packId, [elementId]);
  }

  /** "Changed by <pack>" (spec 22.6): the pack whose override replaces `id`. */
  changedBy(id: string): ElementChangedBy | null {
    const pack = [...this.packs.values()].find((p) => p.elements.get(id)?.summary.override);
    return pack ? { packId: pack.pack.id, packName: pack.pack.name } : null;
  }

  /** The description pane of an overridden element: the override as saved, with "Changed by". */
  overridden(id: string): ElementDetail | null {
    const by = this.changedBy(id);
    const saved = by ? this.own(by.packId).elements.get(id) : undefined;
    return by && saved ? { ...this.preview(by.packId, id, saved.form), changedBy: by } : null;
  }

  updateDraft(draftId: string, form: HomebrewForm): HomebrewDraftView {
    const draft = this.draft(draftId);
    this.touched.add(draftId);
    return this.store(draft.packId, draft.elementId, refreshSubclass(form), draftId);
  }

  /** The engine checks (test character, level sweep, balance hints) need the engine: the mock has none to report. */
  checkDraft(draftId: string): HomebrewProblem[] { this.draft(draftId); return []; }

  saveDraft(draftId: string): HomebrewSaveResult {
    const draft = this.draft(draftId);
    if (draft.problems.some((p) => p.severity === "error")) {
      throw { ...apiError("invalid_argument", "Fix the problems marked in red before saving."), details: draft.problems };
    }
    const own = this.own(draft.packId);
    const { type, name, id: fresh } = this.newIdentity(own, draft.form);
    const k = KINDS.find((x) => x.id === draft.form.kind)!;
    const id = draft.elementId ?? fresh;
    const summary: HomebrewElementSummary = {
      id, name, kind: k.id, elementType: type, parentId: null, edition: listEdition(draft.form), problems: { errors: 0, warnings: 0 }, hasDraft: false, tags: [], text: "",
      copiedText: this.copiedText.has(draftId) || (own.elements.get(id)?.summary.copiedText ?? false),
      ...(this.overrideOf.has(draftId) ? { override: { ...this.overrideOf.get(draftId)!, changed: false } } : {}),
    };
    if (summary.override && !summary.override.srd) summary.copiedText = true;
    const elements = new Map(own.elements).set(id, { summary, form: draft.form });
    const savedWith = (this.companions.get(draftId) ?? []).map((c) => {
      const ck = KINDS.find((x) => x.id === c.kind)!;
      const ctype = ck.elementTypes[0] ?? ck.label;
      const cname = String(c.fields.name ?? "").trim();
      const cid = this.freeId({ ...own, elements }, `ID_${own.code}_${TYPE_TOKENS[ctype] ?? slug(ctype)}_${slug(cname)}`);
      elements.set(cid, { summary: { ...summary, id: cid, name: cname, kind: ck.id, elementType: ctype }, form: c });
      return cid;
    });
    this.companions.delete(draftId);
    const pack = marked({ ...own.pack, elementCount: elements.size, version: bumpPatch(own.pack.version) }, elements);
    this.packs.set(own.pack.id, { ...own, pack, elements });
    this.drafts.delete(draftId);
    this.touched.delete(draftId);
    this.copiedText.delete(draftId);
    this.copySources.delete(draftId);
    this.overrideOf.delete(draftId);
    return { pack: this.putPack(pack), elementIds: [id, ...savedWith] };
  }

  discardDraft(draftId: string): void {
    this.draft(draftId);
    this.companions.delete(draftId);
    this.copySources.delete(draftId);
    this.drafts.delete(draftId);
    this.touched.delete(draftId);
  }

  listDrafts(): HomebrewDraftSummary[] {
    return [...this.drafts.values()].filter((d) => this.touched.has(d.draftId)).map((d) => ({ draftId: d.draftId, packId: d.packId, elementId: d.elementId, kind: d.form.kind, name: String(d.form.fields.name ?? ""),
      edition: listEdition(d.form), updatedAt: d.updatedAt }));
  }

  deleteElements(packId: string, elementIds: string[]): HomebrewDeleteResult {
    const own = this.own(packId);
    const elements = new Map([...own.elements].filter(([id]) => !elementIds.includes(id)));
    const next = this.write({ ...own, elements });
    return { deleted: true, usedBy: [], pack: this.putPack(next.pack), undoId: this.record([[own, next]]) };
  }

  /** Multi-select edits (spec 12.5): edition and tags, a move, or duplicates; one step, undone as one. */
  bulkEdit(packId: string, elementIds: string[], patch: HomebrewBulkPatch): HomebrewBulkResult {
    const own = this.own(packId);
    const chosen = elementIds.map((id) => own.elements.get(id) ?? (() => { throw apiError("not_found", "That element isn't in this pack any more."); })());
    const fields = patch.edition !== undefined || (patch.addTags?.length ?? 0) > 0 || (patch.removeTags?.length ?? 0) > 0;
    if (Number(fields) + Number(!!patch.moveToPackId) + Number(!!patch.duplicate) !== 1) throw apiError("invalid_argument", "Choose one change: the edition and tags, a move, or a duplicate.");
    if (patch.moveToPackId) {
      const target = this.own(patch.moveToPackId);
      if (target.pack.id === packId) throw apiError("invalid_argument", "Those elements are in that pack already.");
      const from = this.write({ ...own, elements: new Map([...own.elements].filter(([id]) => !elementIds.includes(id))) });
      const moved = chosen.map((e) => [e.summary.id, { ...e, form: { ...e.form, fields: { ...e.form.fields, ...("source" in e.form.fields ? { source: target.pack.name } : {}) } } }] as const);
      const to = this.write({ ...target, elements: new Map([...target.elements, ...moved]) });
      return { pack: this.putPack(from.pack), elementIds, undoId: this.record([[own, from], [target, to]]) };
    }
    if (patch.duplicate) {
      const elements = new Map(own.elements);
      const made = chosen.map((e) => {
        const name = `${e.summary.name} (copy)`;
        const id = this.freeId({ ...own, elements }, `${e.summary.id.replace(/_\d+$/, "")}_COPY`);
        elements.set(id, { summary: { ...e.summary, id, name }, form: { ...e.form, fields: { ...e.form.fields, name } } });
        return id;
      });
      const next = this.write({ ...own, elements });
      return { pack: this.putPack(next.pack), elementIds: made, undoId: this.record([[own, next]]) };
    }
    if (patch.edition !== undefined && patch.edition !== null && chosen.some((e) => e.form.kind === "class" && editionOf(e.form) !== patch.edition)) {
      throw { ...apiError("invalid_argument", "Some elements can't take that change, so nothing was changed."), details: [{ severity: "error", message: "A class keeps its edition. Make a new class for the other edition.", path: "fields.edition", elementId: null, code: "form" }] };
    }
    const changed = chosen.flatMap((e) => {
      let form = withTags(e.form, patch.addTags ?? [], patch.removeTags ?? []);
      if (patch.edition !== undefined && editionOf(form) !== patch.edition) form = { ...form, fields: { ...form.fields, edition: patch.edition ?? "" } };
      return form === e.form ? [] : [[e.summary.id, { summary: { ...e.summary, edition: listEdition(form) }, form }] as const];
    });
    if (changed.length === 0) return { pack: own.pack, elementIds: [], undoId: null };
    const next = this.write({ ...own, elements: new Map([...own.elements, ...changed]) });
    return { pack: this.putPack(next.pack), elementIds: changed.map(([id]) => id), undoId: this.record([[own, next]]) };
  }

  /** Undo a delete or a multi-select edit: every pack it wrote as it was, unless one changed since. */
  undo(undoId: string): ContentPack {
    const r = this.undos.get(undoId);
    if (!r) throw apiError("not_found", "That change can't be undone any more.");
    this.undos.delete(undoId);
    if (r.packs.some((p) => this.own(p.before.pack.id).pack.version !== p.versionAfter)) throw apiError("requirement_failed", "The pack changed since, so nothing was undone.");
    const packs = r.packs.map((p) => this.write({ ...p.before, pack: { ...p.before.pack, version: p.versionAfter } }));
    packs.forEach((p) => this.putPack(p.pack));
    return packs[0]!.pack;
  }

  /** Where used (spec 12.3): own elements whose forms name it, and the loaded classes whose list a spell is on. */
  whereUsed(elementId: string, loaded: readonly ElementSummary[]): HomebrewElementRef[] {
    const own = [...this.packs.values()].flatMap((p) => [...p.elements.values()].flatMap(({ summary, form }) => {
      const relation = summary.id === elementId ? null : formUse(form, elementId);
      return relation ? [{ id: summary.id, name: summary.name, type: summary.elementType, packId: p.pack.id, relation }] : [];
    }));
    const saved = [...this.packs.values()].map((p) => p.elements.get(elementId)).find((e) => e !== undefined);
    const spellTags = saved?.form.kind === "spell" ? tagsOf(saved.form)
      : [...(loaded.find((e) => e.id === elementId)?.tags ?? []), ...[...this.packs.values()].flatMap((p) => p.appends.get(elementId) ?? [])];
    const order = ["grants", "selects", "requires", "lists"];
    return [...own, ...this.draftUses(elementId), ...listingClasses(loaded, spellTags)].sort((a, b) => order.indexOf(a.relation) - order.indexOf(b.relation) || a.name.localeCompare(b.name));
  }

  /** The packs' expanded lists of subclasses (spec 12.4): pack id -> subclass id -> the spells the pack put on it (an own
   * subclass's `extends`, or the pack's append for a loaded one). The mock's loaded subclasses have none of their own. */
  private expanded = new Map<string, Map<string, string[]>>();

  /** SPELL LISTS' expanded-list choices: the loaded subclasses of casting classes and every own subclass. */
  expandedListRows(loaded: readonly ElementSummary[]): HomebrewPickerRow[] {
    const casters = new Set(["evoker", "diviner", "life-domain", "light-domain", "circle-of-the-land", "circle-of-the-moon"]);
    const own = [...this.packs.values()].flatMap((p) => [...p.elements.values()].filter(({ form }) => form.kind === "subclass")
      .map(({ summary }): HomebrewPickerRow => ({ id: summary.id, name: summary.name, type: "Expanded Spell List", summary: "", prerequisites: null,
        sourceName: p.pack.name, edition: summary.edition, draft: false })));
    const others = loaded.filter((e) => e.type === "archetype" && casters.has(e.id)).map((e) => ({ ...pickerRow(e), type: "Expanded Spell List" }));
    return [...own, ...others].sort((a, b) => a.name.localeCompare(b.name));
  }

  /** A subclass's expanded list (spec 12.4): every loaded spell and the pack's own, on the list when the pack put it there. */
  expandedList(packId: string, subclassId: string, loaded: readonly ElementSummary[]): HomebrewSpellListView {
    const own = this.own(packId);
    const mine = own.elements.get(subclassId);
    const sub = mine?.summary.name ?? loaded.find((e) => e.id === subclassId && e.type === "archetype")?.name;
    if (!sub) throw apiError("not_found", "That subclass isn't loaded any more.");
    const on = new Set(this.expanded.get(packId)?.get(subclassId) ?? []);
    const spells = [
      ...[...own.elements.values()].filter((e) => e.form.kind === "spell").map(({ summary, form }) => ({
        id: summary.id, name: summary.name, type: "Spell", summary: "", prerequisites: null, sourceName: own.pack.name, edition: summary.edition, draft: false, ...spellRow(form),
      })),
      ...loaded.filter((e) => e.type === "spell").map((e) => ({ ...pickerRow(e), type: "Spell" })),
    ];
    const rows = spells.map((r): HomebrewSpellListRow => ({ ...r, onList: on.has(r.id), removable: on.has(r.id) })).sort((a, b) => a.name.localeCompare(b.name));
    return { listTag: `${sub} expanded list`, expandedOf: subclassId, own: !!mine, count: rows.filter((r) => r.onList).length, rows };
  }

  /** Put spells on a subclass's expanded list and take them off: only what the pack put there comes off. */
  setExpandedList(packId: string, subclassId: string, add: string[], remove: string[], loaded: readonly ElementSummary[]): HomebrewSpellListView {
    const lists = new Map(this.expanded.get(packId) ?? []);
    const now = lists.get(subclassId) ?? [];
    lists.set(subclassId, [...new Set([...now, ...add])].filter((id) => !remove.includes(id)));
    this.expanded.set(packId, lists);
    this.putPack(this.write(this.own(packId)).pack);
    return this.expandedList(packId, subclassId, loaded);
  }

  /** A SPELL LISTS view (spec 12.4): every loaded spell and the pack's own, on the list or not. */
  spellList(packId: string, tag: string, loaded: readonly ElementSummary[]): HomebrewSpellListView {
    const own = this.own(packId);
    if (!tag.trim()) throw apiError("invalid_argument", "Choose a spell list first.");
    const mine = [...own.elements.values()].filter((e) => e.form.kind === "spell").map(({ summary, form }): HomebrewSpellListRow => ({
      id: summary.id, name: summary.name, type: "Spell", summary: "", prerequisites: null, sourceName: own.pack.name, edition: summary.edition, draft: false,
      ...spellRow(form), onList: tagsOf(form).includes(tag), removable: tagsOf(form).includes(tag),
    }));
    const others = loaded.filter((e) => e.type === "spell").map((e) => loadedListRow(e, tag, own.appends.get(e.id) ?? []));
    const rows = [...mine, ...others].sort((a, b) => a.name.localeCompare(b.name));
    return { listTag: tag, expandedOf: null, own: false, count: rows.filter((r) => r.onList).length, rows };
  }

  /** Put spells on a list and take them off: own spells by their lists, loaded ones by the pack's append (HC5). */
  setSpellList(packId: string, tag: string, add: string[], remove: string[], loaded: readonly ElementSummary[]): HomebrewSpellListView {
    const own = this.own(packId);
    const elements = new Map(own.elements);
    const appends = new Map(own.appends);
    for (const [id, on] of [...add.map((id) => [id, true] as const), ...remove.map((id) => [id, false] as const)]) {
      const mine = elements.get(id);
      if (mine) { elements.set(id, { ...mine, form: withTags(mine.form, on ? [tag] : [], on ? [] : [tag]) }); continue; }
      const spell = loaded.find((e) => e.id === id);
      if (!spell) throw apiError("not_found", "That spell isn't loaded any more.");
      const now = appends.get(id) ?? [];
      if (!on && hasTag(spell.tags ?? [], tag)) throw { ...apiError("invalid_argument", "Some spells can't be changed on this list."), details: [{ severity: "error", message: `${spell.name} is on the ${tag} list in its own pack. A homebrew pack can only add to that list.`, path: null, elementId: null, code: "form" }] };
      const next = on ? [...new Set([...now, tag])] : now.filter((t) => t !== tag);
      if (next.length) appends.set(id, next); else appends.delete(id);
    }
    const next = this.write({ ...own, elements, appends });
    this.putPack(next.pack);
    return this.spellList(packId, tag, loaded);
  }

  duplicateElement(packId: string, elementId: string): HomebrewDraftView {
    const saved = this.own(packId).elements.get(elementId);
    if (!saved) throw apiError("not_found", "That element isn't in this pack any more.");
    const form = { ...saved.form, fields: { ...saved.form.fields, name: `${String(saved.form.fields.name)} (copy)` } };
    return this.store(packId, null, form);
  }

  /** Picker rows of every own pack's saved elements, and of the open drafts of `draftId`'s pack (spec 5.1: "plus the current
   * pack's unsaved drafts"). */
  pickerRows(draftId?: string, copying?: "srd" | "any", tags: readonly string[] = []): HomebrewPickerRow[] {
    // the SRD copy sources are offered by the copy pickers only (the other pickers keep the sample content's rows)
    const srd = copying ? MOCK_COPY_SOURCES.map((c) => c.row) : [];
    if (copying === "srd") return srd;
    const draftPack = draftId ? this.drafts.get(draftId)?.packId : undefined;
    // a tag filter (a class option's group, WB-18) keeps the elements carrying one of the tags, spelled exactly (the
    // backend's `homebrew_refs::passes`; a class option's group is its first tag)
    const tagged = (form: HomebrewForm) => tags.length === 0
      || [...tagsOf(form), String(form.fields.group ?? "")].some((t) => tags.includes(t));
    const saved = [...this.packs.values()].flatMap((own) => [...own.elements.values()].filter(({ form }) => tagged(form)).map(({ summary, form }): HomebrewPickerRow => ({
      id: summary.id, name: summary.name, type: summary.elementType, summary: String(form.fields.summary ?? ""), prerequisites: featPrerequisite(form, this.nameOf),
      sourceName: own.pack.name, edition: summary.edition, draft: false, ...spellRow(form),
    })));
    const drafts = [...this.drafts.values()].filter((d) => d.packId === draftPack && d.elementId === null && d.draftId !== draftId && String(d.form.fields.name ?? "").trim() && tagged(d.form))
      .map((d): HomebrewPickerRow => ({ id: `draft:${d.draftId}`, name: String(d.form.fields.name).trim(), type: KINDS.find((k) => k.id === d.form.kind)?.elementTypes[0] ?? "", summary: "", prerequisites: null, sourceName: this.packs.get(d.packId)?.pack.name ?? "", edition: null, draft: true }));
    return [...saved, ...drafts, ...srd, ...CORE_PICKER_ROWS];
  }

  /** The generic form's word lists (WB-19): the open types (the Type combo's own and the saved elements'), or the setter
   * names saved elements of a type use. */
  words(types: readonly string[]): string[] {
    if (types[0] === FEAT_PARAMETER_PICKER) return featParameterWords(types);
    const saved = [...this.packs.values()].flatMap((own) => [...own.elements.values()]);
    // the skill blocks' names (HC16): the skills saved Skill elements add
    if (types.includes(SKILL_NAME_PICKER)) return contentSkillNames(saved.map((e) => e.form));
    if (types.includes(ELEMENT_TYPE_PICKER)) {
      const own = KINDS.find((k) => k.id === "other")?.fields.find((f) => f.key === "type")?.choices?.map((c) => c.value) ?? [];
      const used = saved.filter((e) => e.form.kind === "other").map((e) => e.summary.elementType);
      return [...new Set([...own, ...used.filter((t) => !own.includes(t)).sort()])];
    }
    const sorted = (words: string[]) => [...new Map(words.map((w) => w.trim()).filter(Boolean).map((w) => [w.toLowerCase(), w] as const)).values()]
      .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
    // the backend reads loaded content (`item_categories`, `loaded_tags`); the mock reads the pack's saved elements
    if (types.includes(ITEM_CATEGORY_PICKER)) {
      return sorted(saved.filter((e) => ITEM_KIND_IDS.includes(e.form.kind)).map((e) => String(e.form.fields.category ?? "")));
    }
    if (types[0] === TAG_PICKER) {
      const ofTypes = types.slice(1);
      return sorted(saved.filter((e) => ofTypes.includes(e.summary.elementType))
        .flatMap((e) => (Array.isArray(e.form.fields.tags) ? e.form.fields.tags.map(String) : [])).filter((t) => !t.startsWith("ID_")));
    }
    const ofType = types.filter((t) => t !== SETTER_NAME_PICKER);
    const names = saved.filter((e) => ofType.includes(e.summary.elementType))
      .flatMap((e) => (Array.isArray(e.form.fields.setters) ? e.form.fields.setters as { name?: unknown }[] : []).map((r) => String(r.name ?? "").trim()));
    return [...new Set(names.filter(Boolean))].sort((a, b) => a.localeCompare(b));
  }

  /** A draft for Try it (spec 9): its last view, its kind and its pack's editions. */
  trialDraft(draftId: string): TrialDraft {
    const view = this.draft(draftId);
    const kind = KINDS.find((k) => k.id === view.form.kind);
    if (!kind) throw apiError("invalid_argument", `Unknown kind ${view.form.kind}.`);
    return { view, kind, rulesets: this.own(view.packId).pack.rulesets };
  }

  /** Export pack (spec 14.2): the `.borealis-pack.zip` as bytes (the web view downloads or shares them). */
  exportPack(packId: string, leaveOutCopiedText: boolean): HomebrewExportResult {
    const own = this.own(packId);
    const { bytes, leftOut } = packFile({ pack: own.pack, code: own.code, elements: [...own.elements], appends: [...own.appends] }, leaveOutCopiedText);
    return { fileName: packFileName(own.pack.name), path: null, shared: false, dataBase64: toBase64(bytes), leftOut };
  }

  /** The import preview of a homebrew pack file, or null when `files` are not one (the XML import reads them). */
  previewFile(files: readonly ImportFileInput[]): ImportPreview | null {
    const data = readPackFile(files);
    if (!data) return null;
    const previewId = `prev_hb_${this.nextPreview++}`;
    this.stagedFiles.set(previewId, { fileName: files[0]!.fileName, data });
    const installed = this.packs.has(data.pack.id);
    const codes = new Set([...this.packs.values()].map((p) => p.code));
    const counts = new Map<string, number>();
    for (const [, e] of data.elements) counts.set(e.summary.elementType, (counts.get(e.summary.elementType) ?? 0) + 1);
    return {
      previewId, packName: data.pack.name, packId: data.pack.id, fileCount: 1, converted: data.elements.length, skipped: 0,
      counts: [...counts].map(([type, count]): TypeCount => ({ type, count })), summary: { errors: 0, warnings: 0, infos: 0 },
      diagnostics: [], diagnosticsTruncated: false, unresolved: [], unresolvedTotal: 0, conditions: [], conditionsTotal: 0,
      replacesExisting: installed, books: [],
      homebrew: { code: data.code, codeTaken: !installed && codes.has(data.code), copyCode: copyCode(data.code, codes), copyName: `${data.pack.name} (copy)`,
        // the mock keeps no characters' choices to scan (as its delete scan)
        replaceUsedBy: [], replaceDrops: [] },
    };
  }

  hasStagedFile(previewId: string): boolean { return this.stagedFiles.has(previewId); }
  cancelFile(previewId: string): void { this.stagedFiles.delete(previewId); }

  /** Install a previewed pack file: as it is (replacing an installed pack of its id) or as a copy (a new code and ids). */
  installFile(previewId: string, asCopy: boolean): ImportPackResult {
    const staged = this.stagedFiles.get(previewId);
    if (!staged) throw apiError("invalid_argument", "This preview has expired. Choose the files again.");
    this.stagedFiles.delete(previewId);
    const { data } = staged;
    const codes = new Set([...this.packs.values()].map((p) => p.code));
    if (!asCopy && !this.packs.has(data.pack.id) && codes.has(data.code)) {
      throw apiError("invalid_argument", `Another pack already uses the code ${data.code}. Import it as a copy instead.`);
    }
    const next = asCopy ? copyOf(data, copyCode(data.code, codes), `${data.pack.name} (copy)`) : data;
    const pack: ContentPack = { ...next.pack, elementCount: next.elements.length };
    this.packs.set(pack.id, { pack, code: next.code, elements: new Map(next.elements), appends: new Map(next.appends) });
    return { pack: this.putPack(pack), report: { fileName: staged.fileName, format: "borealis-pack", converted: next.elements.length, skipped: 0, issues: [] } };
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
      ...itemFacts(form),
      ...(typeof form.fields.enhancement === "number" && form.fields.enhancement > 0 ? [{ label: "Bonus", value: `+${form.fields.enhancement}` }] : []),
      ...(form.kind === "spell" ? SPELL_FACTS.filter(([key]) => text(key)).map(([key, label]) => ({ label, value: text(key) })) : []),
    ];
    const description = descriptionHtml(form.fields.description) + (form.kind === "creature" ? creatureHtml(form) : "");
    return {
      id, name: text("name") || "Unnamed", type: PREVIEW_TYPES[form.kind] ?? "option", source: own.pack.sourceIds[0] ?? own.pack.id,
      sourceName: own.pack.name, shortDescription: "", prerequisites: featPrerequisite(form, this.nameOf), rulesets: own.pack.rulesets,
      descriptionHtml: description, facts: form.kind === "creature" ? creatureFacts(form) : k ? facts : [], grants: [],
    };
  }

  private undos = new Map<string, UndoRecord>();
  private nextUndo = 1;

  /** The pack with a new version (every write bumps its patch number), stored. */
  private write(own: OwnPack): OwnPack {
    const next = { ...own, pack: marked({ ...own.pack, elementCount: own.elements.size, version: bumpPatch(own.pack.version) }, own.elements) };
    this.packs.set(own.pack.id, next);
    return next;
  }

  /** Keep an undoable change: each pack as it was, with the version the change wrote. */
  private record(pairs: [OwnPack, OwnPack][]): string {
    const id = `undo-${this.nextUndo++}`;
    this.undos.set(id, { packs: pairs.map(([before, after]) => ({ before, versionAfter: after.pack.version })) });
    return id;
  }

  /** The DATA view (spec 12.1) of a draft: the element the mock would store. */
  private generated(packId: string, elementId: string | null, form: HomebrewForm, draftId: string): HomebrewDraftView["generated"] {
    const own = this.own(packId);
    const k = KINDS.find((x) => x.id === form.kind);
    const type = k?.elementTypes[0] ?? (String(form.fields.type ?? "").trim() || "Element");
    const id = elementId ?? `ID_${own.code}_${TYPE_TOKENS[type] ?? slug(type)}_${slug(String(form.fields.name ?? ""))}`;
    const description = descriptionHtml(form.fields.description);
    const copied = this.copySources.get(draftId);
    if (copied) return copiedGenerated(copied.generated, { id, name: String(form.fields.name ?? "").trim(), source: own.pack.name, description });
    return generatedOf(form, id, type, own.pack.name, description);
  }

  /** `savedWith` of a draft's view: the names of the forms a deep copy saves with it (the backend's `HomebrewDraftView`). */
  private savedWith(draftId: string): { savedWith?: string[] } {
    const names = (this.companions.get(draftId) ?? []).map((c) => String(c.fields.name ?? "").trim()).filter(Boolean);
    return names.length ? { savedWith: names } : {};
  }

  /** A new element's type, name and the id it gets on its first save (the backend's provisional id). */
  private newIdentity(own: OwnPack, form: HomebrewForm): { type: string; name: string; id: string } {
    const k = KINDS.find((x) => x.id === form.kind)!;
    const type = k.elementTypes[0] ?? (String(form.fields.type ?? "").trim() || k.label);
    const name = String(form.fields.name ?? "").trim();
    return { type, name, id: this.freeId(own, `ID_${own.code}_${TYPE_TOKENS[type] ?? slug(type)}_${slug(name)}`) };
  }

  /** H0eFo: the edited new drafts (never saved) that use the element, as the backend lists them; a draft that can't be
   * written yet (it has errors) is left out, as the backend's. */
  private draftUses(elementId: string): HomebrewElementRef[] {
    return [...this.drafts.values()]
      .filter((d) => d.elementId === null && this.touched.has(d.draftId) && !d.problems.some((p) => p.severity === "error"))
      .flatMap((d) => {
        const relation = formUse(d.form, elementId);
        if (!relation) return [];
        const { type, name, id } = this.newIdentity(this.own(d.packId), d.form);
        return [{ id, name, type, packId: d.packId, relation, draftId: d.draftId }];
      });
  }

  /** The statistic keys of the skills saved Skill elements add (the backend's `form::skill_keys` over the loaded store;
   * a draft not saved is no skill yet). */
  private loadedSkillKeys(): string[] {
    const forms = [...this.packs.values()].flatMap((own) => [...own.elements.values()].map((e) => e.form));
    return contentSkillNames(forms).flatMap((name) => skillKey(name) ?? []);
  }

  private freeId(own: OwnPack, base: string): string {
    if (!own.elements.has(base)) return base;
    for (let n = 2; ; n += 1) if (!own.elements.has(`${base}_${n}`)) return `${base}_${n}`;
  }

  private store(packId: string, elementId: string | null, form: HomebrewForm, draftId = `draft-${this.nextDraft++}`): HomebrewDraftView {
    const skills = this.loadedSkillKeys();
    const problems = [
      ...formProblems(form, MOCK_BLOCK_CATALOG, elementId, KINDS.find((k) => k.id === form.kind), skills),
      ...spellListProblems(form, elementId, form.kind === "spell" ? this.spellLists() : []),
      ...subclassProblems(form, elementId),
      ...backgroundProblems(form, elementId),
      ...creatureProblems(form, elementId),
      // a child's problems, at its path (`children.0.fields.name`)
      ...form.children.flatMap((c, i) => formProblems(c, MOCK_BLOCK_CATALOG, null, KINDS.find((k) => k.id === c.kind), skills)
        .map((p) => ({ ...p, path: `children.${i}.${p.path ?? ""}`.replace(/\.$/, "") }))),
    ];
    const view: HomebrewDraftView = {
      draftId, packId, elementId, form, preview: this.preview(packId, elementId ?? draftId, form), problems, updatedAt: now(), baseChanged: false,
      generated: this.generated(packId, elementId, form, draftId), ...this.savedWith(draftId),
      ...(this.overrideOf.has(draftId) ? { override: this.overrideOf.get(draftId)! } : {}),
    };
    this.drafts.set(draftId, view);
    return view;
  }
}


/** A form's own edition, or null for the pack's. */
function editionOf(form: HomebrewForm): BookRuleset | null {
  const e = form.fields.edition;
  return e === "dnd2014" || e === "dnd2024" || e === "any" ? e : null;
}

/** The element list's edition column: a single edition, else null (the pack's, or either). */
const listEdition = (form: HomebrewForm): Ruleset | null => { const e = editionOf(form); return e === "any" ? null : e; };

/** The form and its children name the pack's book as their source (the backend's `with_source`). */
function withSource(form: HomebrewForm, source: string): HomebrewForm {
  const fields = "source" in form.fields ? { ...form.fields, source } : form.fields;
  return { ...form, fields, children: form.children.map((c) => withSource(c, source)) };
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
