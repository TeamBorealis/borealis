import type {
  CompendiumFacets, FiveToolsImportReport, FiveToolsImportRequest, FiveToolsScan, CompendiumQuery, ContentIndex, ContentPack, ContentProgress, ContentSource, ElementDetail, ElementSummary,
  ImportPackRequest, ImportPackResult, ItemQuery, ItemSummary, MigrationIssue, Page, UpdateIndexesResult,
} from "../types";
import { ELEMENTS, ITEMS, getElement } from "./content";
import type { MockElement } from "./data-types";
import { apiError } from "./errors";
import { SOURCE_2014, SOURCE_2024 } from "./content";

const TYPE_LABELS: Record<string, string> = {
  race: "Races", subrace: "Subraces", class: "Classes", multiclass: "Multiclass", archetype: "Subclasses", "fighting-style": "Fighting styles",
  background: "Backgrounds", feat: "Feats", spell: "Spells", weapon: "Weapons", armor: "Armor", item: "Gear", "magic-item": "Magic items",
  skill: "Skills", language: "Languages", asi: "Ability improvements", list: "Characteristics",
};

const stripHtml = (s: string): string => s.replace(/<[^>]+>/g, "");

/** Compendium, packs, sources and content indexes for the mock backend. */
export class ContentService {
  private extra: MockElement[] = [];
  private imported: ContentPack[] = [];
  private disabled = new Set<string>();
  private indexes: ContentIndex[] = [];

  constructor(private emit: (p: ContentProgress) => void, private reloaded: () => void, private emitImport: (p: ContentProgress) => void = () => undefined) {}

  private all(): MockElement[] {
    return [...ELEMENTS, ...this.extra].filter((e) => !this.disabled.has(this.packOf(e.source)));
  }

  private packOf(source: string): string {
    return source === SOURCE_2014 ? "pack-srd-51" : source === SOURCE_2024 ? "pack-srd-52" : this.imported.find((p) => p.sourceIds.includes(source))?.id ?? "pack-homebrew";
  }

  private summary(e: MockElement): ElementSummary {
    const { description: _d, facts: _f, grants: _g, ...rest } = e;
    return rest;
  }

  search(q: CompendiumQuery): Page<ElementSummary> {
    const text = q.query.trim().toLowerCase();
    const hits = this.all().filter((e) => {
      if (q.types?.length && !q.types.includes(e.type)) return false;
      if (q.sources?.length && !q.sources.includes(e.source)) return false;
      if (q.ruleset && q.ruleset !== "any" && !e.rulesets.includes(q.ruleset)) return false;
      if (!text) return !["multiclass", "asi", "list"].includes(e.type);
      return e.name.toLowerCase().includes(text) || e.shortDescription.toLowerCase().includes(text) || (e.tags ?? []).some((t) => t.toLowerCase().includes(text));
    }).sort((a, b) => a.name.localeCompare(b.name));
    return { total: hits.length, items: hits.slice(q.offset, q.offset + q.limit).map((e) => this.summary(e)) };
  }

  element(id: string): ElementDetail {
    const e = this.all().find((x) => x.id === id) ?? getElement(id);
    // Like the real backend: list-rule entries (personality traits, ...) are text choices, not elements.
    if (!e || e.type === "list") throw apiError("not_found", "That element does not exist.");
    return { ...this.summary(e), descriptionHtml: e.description, facts: e.facts, grants: e.grants };
  }

  facets(): CompendiumFacets {
    const count = (key: (e: MockElement) => string) => {
      const m = new Map<string, number>();
      for (const e of this.all()) m.set(key(e), (m.get(key(e)) ?? 0) + 1);
      return m;
    };
    const types = [...count((e) => e.type)].filter(([id]) => TYPE_LABELS[id]).map(([id, n]) => ({ id, label: TYPE_LABELS[id]!, count: n }));
    const sources = [...count((e) => e.source)].map(([id, n]) => ({ id, label: this.all().find((e) => e.source === id)!.sourceName, count: n }));
    return { types, sources };
  }

  items(q: ItemQuery): Page<ItemSummary> {
    const text = (q.query ?? "").toLowerCase();
    const hits = ITEMS.filter((i) => (!q.category || i.category === q.category) && (!text || getElement(i.id)!.name.toLowerCase().includes(text)))
      .map((i) => ({ ...this.summary(getElement(i.id)!), category: i.category, costCp: i.costCp, weight: i.weight, requiresAttunement: !!i.attune }));
    const sorted = hits.sort((a, b) => a.name.localeCompare(b.name));
    return { total: sorted.length, items: sorted.slice(q.offset ?? 0, (q.offset ?? 0) + (q.limit ?? 100)) };
  }

  packs(): ContentPack[] {
    const n = (s: string) => ELEMENTS.filter((e) => e.source === s).length;
    const base: ContentPack[] = [
      { id: "pack-srd-51", name: "Sample Reference 5.1", version: "1.0.0", kind: "bundled", rulesets: ["dnd2014"], elementCount: n(SOURCE_2014), enabled: !this.disabled.has("pack-srd-51"), license: "CC-BY-4.0", attribution: "Sample content written for Borealis.", sourceIds: [SOURCE_2014] },
      { id: "pack-srd-52", name: "Sample Reference 5.2", version: "1.0.0", kind: "bundled", rulesets: ["dnd2024"], elementCount: n(SOURCE_2024), enabled: !this.disabled.has("pack-srd-52"), license: "CC-BY-4.0", attribution: "Sample content written for Borealis.", sourceIds: [SOURCE_2024] },
    ];
    const home = this.extra.length
      ? [{ id: "pack-homebrew", name: "My Homebrew", version: "0.1.0", kind: "homebrew" as const, rulesets: ["dnd2014", "dnd2024"] as ContentPack["rulesets"], elementCount: this.extra.length, enabled: !this.disabled.has("pack-homebrew"), license: "User", attribution: "", sourceIds: ["sample-homebrew"] }]
      : [];
    return [...base, ...home, ...this.imported.map((p) => ({ ...p, enabled: !this.disabled.has(p.id) }))];
  }

  sources(): ContentSource[] {
    return this.packs().flatMap((p) => p.sourceIds.map((id) => ({
      id, name: this.all().find((e) => e.source === id)?.sourceName ?? id, group: p.name, packId: p.id,
      elementCount: ELEMENTS.filter((e) => e.source === id).length + this.extra.filter((e) => e.source === id).length,
      description: `Content from ${p.name}.`, playtest: false,
    })));
  }

  setEnabled(packId: string, enabled: boolean): ContentPack[] {
    if (enabled) this.disabled.delete(packId);
    else this.disabled.add(packId);
    return this.packs();
  }

  remove(packId: string): ContentPack[] {
    const pack = this.packs().find((p) => p.id === packId);
    if (!pack || pack.kind === "bundled") throw apiError("invalid_argument", "Bundled packs cannot be removed.");
    this.imported = this.imported.filter((p) => p.id !== packId);
    if (packId === "pack-homebrew") this.extra = [];
    return this.packs();
  }

  import(req: ImportPackRequest): ImportPackResult {
    let text: string;
    try { text = atob(req.contentBase64); } catch { throw apiError("invalid_argument", "File content is not valid base64."); }
    const format = req.format ?? (req.fileName.toLowerCase().endsWith(".xml") ? "xml" : "json");
    const issues: MigrationIssue[] = [];
    let names: string[] = [];
    if (format === "xml") {
      names = [...text.matchAll(/<element\s+[^>]*name="([^"]+)"/g)].map((m) => m[1]!);
      if (!names.length) issues.push({ severity: "error", element: null, message: "No <element> records were found." });
      if (/<spellcasting/i.test(text)) issues.push({ severity: "warning", element: null, message: "Spellcasting blocks were converted with default progression." });
    } else {
      try {
        const doc = JSON.parse(text) as { elements?: { name?: string }[] };
        names = (doc.elements ?? []).map((e) => e.name ?? "Unnamed");
        if (!doc.elements) issues.push({ severity: "error", element: null, message: 'Expected a top-level "elements" array.' });
      } catch (e) {
        issues.push({ severity: "error", element: null, message: `Invalid JSON: ${(e as Error).message}` });
      }
    }
    const report = { fileName: req.fileName, format, converted: names.length, skipped: 0, issues };
    if (!names.length) return { pack: null, report };
    const id = `pack-import-${this.imported.length + 1}`;
    const pack: ContentPack = { id, name: req.fileName.replace(/\.[^.]+$/, ""), version: "1.0.0", kind: "imported", rulesets: ["dnd2014"], elementCount: names.length, enabled: true, license: "User content", attribution: "", sourceIds: [`import-${id}`] };
    this.imported = [...this.imported, pack];
    this.reloaded();
    return { pack, report };
  }

  addImportedPack(name: string, count: number): ContentPack {
    const id = `pack-import-${this.imported.length + 1}`;
    const pack: ContentPack = { id, name, version: "1.0.0", kind: "imported", rulesets: ["dnd2024"], elementCount: count, enabled: true, license: "User content", attribution: "", sourceIds: [`import-${id}`], redistributable: false };
    this.imported = [...this.imported, pack];
    this.reloaded();
    return pack;
  }

  addHomebrew(json: Record<string, unknown>): ContentPack {
    const name = String(json.name ?? "Unnamed");
    const el: MockElement = {
      id: String(json.id ?? name), type: (json.type as MockElement["type"]) ?? "option", name, source: "sample-homebrew", sourceName: "Sample Homebrew",
      shortDescription: stripHtml(String(json.description ?? "")).slice(0, 120), prerequisites: null, rulesets: ["dnd2014", "dnd2024"],
      description: String(json.description ?? ""), facts: [], grants: [], ...(typeof json.level === "number" ? { level: json.level } : {}),
    };
    this.extra = [...this.extra.filter((e) => e.id !== el.id), el];
    this.reloaded();
    return this.packs().find((p) => p.id === "pack-homebrew")!;
  }

  listIndexes(): ContentIndex[] { return this.indexes; }

  addIndex(url: string): ContentIndex[] {
    if (!/^https?:\/\/.+\.index$/i.test(url)) throw apiError("invalid_argument", "An index URL must start with http(s) and end with .index.");
    if (/\/user\.index$/i.test(url)) throw apiError("invalid_argument", 'The name "user" is reserved for your personal content folder.');
    this.indexes = [...this.indexes.filter((i) => i.url !== url), { url, name: url.split("/").pop()!.replace(".index", ""), lastChecked: null, fileCount: 3, updatesAvailable: 3, requiresNewerApp: false, verified: false }];
    return this.indexes;
  }

  removeIndex(url: string): ContentIndex[] {
    this.indexes = this.indexes.filter((i) => i.url !== url);
    return this.indexes;
  }

  async update(): Promise<UpdateIndexesResult> {
    const total = this.indexes.reduce((s, i) => s + i.updatesAvailable, 0);
    this.emit({ phase: "checking", done: 0, total, message: "Checking indexes" });
    for (let i = 1; i <= total; i++) this.emit({ phase: "downloading", done: i, total, message: `Downloading file ${i} of ${total}` });
    this.indexes = this.indexes.map((i) => ({ ...i, updatesAvailable: 0, lastChecked: new Date().toISOString() }));
    this.emit({ phase: "reloading", done: total, total, message: "Reloading content" });
    this.reloaded();
    this.emit({ phase: "done", done: total, total, message: "Content is up to date" });
    return { updated: total, failed: [], reloaded: true };
  }

  scan5etools(path: string): FiveToolsScan {
    if (!path.trim() || /missing|nonexistent/i.test(path)) throw apiError("not_found", "No 5etools data folder was found at that path.");
    return { sources: [
      { abbr: "srd", name: "SRD 5.1 (2014)", ruleset: "dnd2014", counts: { class: 4, spell: 24, item: 12 } },
      { abbr: "srd52", name: "SRD 5.2.1 (2024)", ruleset: "dnd2024", counts: { class: 4, spell: 30, feat: 8 } },
      { abbr: "basicRules", name: "Basic Rules (2014)", ruleset: "dnd2014", counts: { feat: 6, spell: 10 } },
    ] };
  }

  import5etools(folder: string, req: FiveToolsImportRequest): FiveToolsImportReport {
    const { sources } = this.scan5etools(folder);
    const picked = sources.filter((s) => req.sources.includes(s.abbr));
    if (!picked.length) throw apiError("invalid_argument", "Select at least one source book.");
    const total = picked.length;
    const packs = picked.map((src, i) => {
      this.emitImport({ phase: "converting", done: i, total, message: `Converting ${src.name}` });
      const id = `pack-5et-${src.abbr.toLowerCase()}`;
      const count = Object.values(src.counts).reduce((a, b) => a + b, 0);
      const pack: ContentPack = { id, name: src.name, version: "local", kind: "local-import", rulesets: [src.ruleset], elementCount: count, enabled: true,
        license: "Local import", attribution: "Converted from the user's own data folder.", sourceIds: [`5et-${src.abbr}`], redistributable: false };
      this.imported = [...this.imported.filter((p) => p.id !== id), pack];
      return pack;
    });
    this.emitImport({ phase: "done", done: total, total, message: "Import finished" });
    this.reloaded();
    const converted = packs.reduce((n, p) => n + p.elementCount, 0);
    return { converted, packs,
      partial: [{ element: "Sample subclass", reason: "Spellcasting progression was converted with defaults." }],
      skipped: [{ element: "Sample vehicle", reason: "Vehicles are not supported." }] };
  }

  clearDownloaded(): ContentPack[] {
    this.imported = this.imported.filter((p) => p.kind !== "downloaded");
    return this.packs();
  }
}

