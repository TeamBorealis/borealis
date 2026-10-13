import type { ImportDraft, ImportElement, ImportEvent, ImportKind, ImportStartRequest } from "../types";
import { apiError } from "./errors";

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const PARENTS = [{ classId: "class-barbarian", name: "Barbarian" }, { classId: "class-fighter", name: "Fighter" }, { classId: "class-wizard", name: "Wizard" }];

function el(id: string, type: string, name: string, level: number | undefined, status: ImportElement["status"], excerpt: string, extra: Partial<ImportElement> = {}): ImportElement {
  return { id, type, name, ...(level ? { level } : {}), status, sourceExcerpt: excerpt, json: { id, type, name, ...(level ? { level } : {}), description: excerpt }, balanceFlags: [], validationIssues: [], ...extra };
}

function runebinder(): ImportElement[] {
  return [
    el("runebinder", "class", "Runebinder", undefined, "ok", "Hit Die d8. Saving throws: Intelligence, Wisdom."),
    el("runebinder-spellcasting", "spellcasting", "Runebinder Spellcasting", 1, "ok", "You know two cantrips and prepare spells from the Runebinder list."),
    el("rune-etching", "class_feature", "Rune Etching", 2, "repaired", "Etch a rune on a weapon as a bonus action.", { validationIssues: ["Fixed: usage 'prof bonus per rest' was not a valid recharge; set to long rest."] }),
    el("rune-ward", "class_feature", "Rune Ward", 5, "ok", "Reaction: gain resistance to one damage type until your next turn."),
    el("glyph-mastery", "class_feature", "Glyph Mastery", 11, "ok", "Your etched runes also deal 1d6 force damage."),
    el("master-runebinder", "class_feature", "Master Runebinder", 20, "needs_review", "Once per day, rewrite the world: cast any spell without a slot.", { balanceFlags: ["Capstone has no cost; similar capstones are limited to once per long rest."], validationIssues: ["Spell level cap missing; assumed 9."] }),
  ];
}

function subclass(): ImportElement[] {
  return [
    el("path-of-the-stormcaller", "subclass", "Path of the Stormcaller", 3, "ok", "Primal path of storm and thunder."),
    el("storm-aura", "class_feature", "Storm Aura", 3, "ok", "Emanation of lightning while raging."),
  ];
}

function generic(kind: ImportKind, name: string): ImportElement[] {
  return [el(`imported-${kind}`, kind, name, undefined, "ok", `${name} as written in the source.`)];
}

export function detect(req: ImportStartRequest): ImportDraft["detected"] {
  const text = req.sources.map((s) => (s.kind === "text" ? s.text : s.kind === "bytes" ? s.name : "")).join(" ").toLowerCase();
  const kind: ImportKind = req.hint?.kind ?? (text.includes("subclass") || text.includes("path of") ? "subclass" : text.includes("feat") ? "feat" : text.includes("spell") ? "spell" : "class");
  const names = kind === "class" ? ["Runebinder"] : kind === "subclass" ? ["Path of the Stormcaller"] : ["Imported " + kind];
  return { kinds: [kind], edition: req.hint?.edition ?? "dnd2024", names, confidence: req.hint?.kind ? 0.99 : 0.86 };
}

/** Simulates the background AI pipeline: staged events, one repaired and one needs_review element. */
export class MockImport {
  private drafts = new Map<string, ImportDraft>();
  private seq = 0;
  constructor(private emit: (e: ImportEvent) => void, private stepMs: number) {}

  async start(req: ImportStartRequest): Promise<{ draftId: string }> {
    if (!req.sources.length) throw apiError("invalid_argument", "Nothing to import.");
    const draftId = `draft-${++this.seq}`;
    const detected = detect(req);
    const kind = detected.kinds[0]!;
    const needsParent = kind === "subclass" && !req.hint?.parentClassId;
    const draft: ImportDraft = {
      draftId, stage: "detect", detected, elements: [], sweep: { levelsPassed: 0, errors: [] },
      suggestedPackName: `${detected.names[0]} (imported)`, suggestedPrefix: "imp",
      ...(needsParent ? { parentChoice: { candidates: PARENTS } } : {}),
      ...(req.hint?.parentClassId ? { parentClassId: req.hint.parentClassId } : {}),
    };
    this.drafts.set(draftId, draft);
    void this.run(draftId, kind, detected.names[0]!);
    return { draftId };
  }

  private put(id: string, patch: Partial<ImportDraft>): ImportDraft {
    const next = { ...this.drafts.get(id)!, ...patch };
    this.drafts.set(id, next);
    return next;
  }
  private send(draftId: string, stage: ImportEvent["stage"], progress: number, message: string, element?: ImportElement) {
    this.put(draftId, { stage });
    this.emit({ draftId, stage, progress, message, ...(element ? { element } : {}) });
  }

  private async run(draftId: string, kind: ImportKind, name: string): Promise<void> {
    await sleep(this.stepMs);
    this.send(draftId, "detect", 0.1, `Looks like a ${kind}: ${name}`);
    await sleep(this.stepMs);
    this.send(draftId, "plan", 0.25, kind === "class" ? "Planning levels 1 to 20, features and spellcasting" : "Planning the pieces");
    const els = kind === "class" ? runebinder() : kind === "subclass" ? subclass() : generic(kind, name);
    for (let i = 0; i < els.length; i++) {
      await sleep(this.stepMs);
      this.put(draftId, { elements: [...this.drafts.get(draftId)!.elements, els[i]!] });
      this.send(draftId, "generate", 0.3 + (0.4 * (i + 1)) / els.length, `Writing ${els[i]!.name}`, els[i]);
    }
    await sleep(this.stepMs);
    const levels = kind === "class" ? 20 : 0;
    this.send(draftId, "validate", 0.85, kind === "class" ? "Playing a test character from level 1 to 20" : "Checking against the rules engine");
    this.put(draftId, { sweep: { levelsPassed: levels, errors: [] } });
    if (els.some((e) => e.status === "repaired")) { await sleep(this.stepMs); this.send(draftId, "repair", 0.93, "Fixed a small mistake automatically"); }
    await sleep(this.stepMs);
    this.send(draftId, "ready", 1, "Ready for your review");
  }

  get(draftId: string): ImportDraft {
    const d = this.drafts.get(draftId);
    if (!d) throw apiError("not_found", "That import no longer exists.");
    return d;
  }

  chooseParent(draftId: string, classId: string): ImportDraft {
    const d = this.get(draftId);
    if (!d.parentChoice?.candidates.some((c) => c.classId === classId)) throw apiError("invalid_argument", "Unknown parent class.");
    const { parentChoice: _drop, ...rest } = d;
    void _drop;
    this.drafts.set(draftId, { ...rest, parentClassId: classId });
    return this.drafts.get(draftId)!;
  }

  async revise(draftId: string, instruction: string, elementId?: string): Promise<void> {
    const d = this.get(draftId);
    if (!instruction.trim()) throw apiError("invalid_argument", "Tell the AI what to change.");
    const target = d.elements.find((e) => e.id === elementId) ?? d.elements.find((e) => e.status === "needs_review") ?? d.elements[0];
    if (!target) return;
    void (async () => {
      await sleep(this.stepMs);
      this.send(draftId, "repair", 0.5, `Rewriting ${target.name}`);
      await sleep(this.stepMs);
      const fixed: ImportElement = { ...target, status: "ok", balanceFlags: [], validationIssues: [], json: { ...target.json, revision: instruction } };
      this.put(draftId, { elements: d.elements.map((e) => (e.id === target.id ? fixed : e)) });
      this.send(draftId, "repair", 0.8, `Updated ${target.name}`, fixed);
      await sleep(this.stepMs);
      this.send(draftId, "ready", 1, "Ready for your review");
    })();
  }

  updateElement(draftId: string, elementId: string, json: Record<string, unknown>): ImportDraft {
    const d = this.get(draftId);
    if (!d.elements.some((e) => e.id === elementId)) throw apiError("not_found", "Unknown element.");
    const issues = typeof json.name === "string" && json.name.trim() ? [] : ["Missing name."];
    const elements = d.elements.map((e) => (e.id === elementId ? { ...e, json, name: typeof json.name === "string" && json.name ? json.name : e.name, validationIssues: issues, status: issues.length ? ("failed" as const) : ("ok" as const), balanceFlags: [] } : e));
    return this.put(draftId, { elements });
  }

  finalize(draftId: string, packName: string, prefix: string): { name: string; count: number; prefix: string } {
    const d = this.get(draftId);
    if (d.parentChoice) throw apiError("invalid_argument", "Choose the parent class first.");
    if (d.elements.some((e) => e.status === "failed")) throw apiError("content_error", "Fix the failed elements first.");
    if (!packName.trim() || !/^[a-z0-9-]{1,8}$/.test(prefix)) throw apiError("invalid_argument", "Pack name and a short lowercase prefix are required.");
    this.drafts.delete(draftId);
    return { name: packName.trim(), count: d.elements.length, prefix };
  }

  discard(draftId: string): void { this.drafts.delete(draftId); }
}
