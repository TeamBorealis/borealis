import type {
  ApiError, BookRuleset, ContentPack, HomebrewDeleteResult, HomebrewDraftSummary, HomebrewDraftView, HomebrewElementSummary, HomebrewFieldSpec,
  HomebrewForm, HomebrewKindDescriptor, HomebrewKindId, HomebrewPackPatch, HomebrewPackRequest, HomebrewProblem, HomebrewSaveResult, Ruleset,
} from "../types";
import { apiError } from "./errors";

/** The backend's answer for a creator command whose work item has not landed (contract: `details.notImplemented`). */
export function notImplemented(command: string): ApiError {
  return { code: "requirement_failed", message: `The homebrew creator can't do this yet (${command} is not implemented).`, details: { notImplemented: command } };
}

const GENERAL = { id: "general", label: "GENERAL" };
const DESCRIPTION = { id: "description", label: "DESCRIPTION" };
const RULES = { id: "rules", label: "RULES" };

const field = (key: string, label: string, control: HomebrewFieldSpec["control"], tab: string, bindings: string[], required = false): HomebrewFieldSpec =>
  ({ key, label, control, tab, bindings, required });

const SHARED: HomebrewFieldSpec[] = [
  field("name", "Name", "text", "general", ["name"], true),
  field("edition", "Edition", "combo", "general", ["ruleset"]),
  field("tags", "Tags", "tags", "general", ["supports"]),
  field("description", "Description", "richText", "description", ["description"]),
];

const kind = (id: string, label: string, pluralLabel: string, elementType: string, description: string, extra: HomebrewFieldSpec[] = []): HomebrewKindDescriptor => ({
  id, label, pluralLabel, description, group: "main", elementTypes: [elementType], parentKinds: [], editions: ["dnd2014", "dnd2024"],
  tabs: [GENERAL, RULES, DESCRIPTION], fields: [...SHARED, ...extra],
});

/** Sample forms for the web mock; the real descriptors come from `borealis-authoring` (WB-04). */
const KINDS: HomebrewKindDescriptor[] = [
  kind("spell", "Spell", "Spells", "Spell", "A spell for class spell lists.", [
    { ...field("level", "Level", "number", "general", ["setter:level", "data.level"], true), min: 0, max: 9 },
    field("school", "School", "combo", "general", ["setter:school", "data.school"], true),
  ]),
  kind("feat", "Feat", "Feats", "Feat", "A feat characters can take.", [field("category", "Category", "combo", "general", ["setter:category", "data.category"])]),
  kind("language", "Language", "Languages", "Language", "A language characters can learn."),
];

const TYPE_TOKENS: Record<string, string> = { Spell: "SPELL", Feat: "FEAT", Language: "LANGUAGE" };
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
    const drafted = new Set([...this.drafts.values()].map((d) => d.elementId));
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
    const type = k.elementTypes[0] ?? k.label;
    const name = String(draft.form.fields.name).trim();
    const id = draft.elementId ?? this.freeId(own, `ID_${own.code}_${TYPE_TOKENS[type] ?? slug(type)}_${slug(name)}`);
    const summary: HomebrewElementSummary = { id, name, kind: k.id, elementType: type, parentId: null, edition: null, problems: { errors: 0, warnings: 0 }, hasDraft: false };
    const elements = new Map(own.elements).set(id, { summary, form: draft.form });
    const pack = { ...own.pack, elementCount: elements.size, version: bumpPatch(own.pack.version) };
    this.packs.set(own.pack.id, { ...own, pack, elements });
    this.drafts.delete(draftId);
    return { pack: this.putPack(pack), elementIds: [id] };
  }

  discardDraft(draftId: string): void {
    this.draft(draftId);
    this.drafts.delete(draftId);
  }

  listDrafts(): HomebrewDraftSummary[] {
    return [...this.drafts.values()].map((d) => ({ draftId: d.draftId, packId: d.packId, elementId: d.elementId, kind: d.form.kind, name: String(d.form.fields.name ?? ""), updatedAt: d.updatedAt }));
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

  private freeId(own: OwnPack, base: string): string {
    if (!own.elements.has(base)) return base;
    for (let n = 2; ; n += 1) if (!own.elements.has(`${base}_${n}`)) return `${base}_${n}`;
  }

  private store(packId: string, elementId: string | null, form: HomebrewForm, draftId = `draft-${this.nextDraft++}`): HomebrewDraftView {
    const named = String(form.fields.name ?? "").trim() !== "";
    const problems: HomebrewProblem[] = named ? [] : [{ severity: "error", message: "Give it a name.", path: "fields.name", elementId, code: "form.required" }];
    const view: HomebrewDraftView = {
      draftId, packId, elementId, form, preview: null, problems, updatedAt: now(), baseChanged: false,
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
