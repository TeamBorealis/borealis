import type { ImportDraft, ImportElement, ImportElementReview, ImportEvent, ImportKind, ImportReviewCounts, ImportSourcePassage, ImportStartRequest } from "../types";
import { apiError } from "./errors";

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const PARENTS = [{ classId: "class-barbarian", name: "Barbarian" }, { classId: "class-fighter", name: "Fighter" }, { classId: "class-wizard", name: "Wizard" }];

const CLASS_SOURCE = [
  "# Stonework Compendium (synthetic sample)",
  "",
  "## Class: Runebinder",
  "Hit Die d8. Saving throws: Intelligence, Wisdom.",
  "You know two cantrips and prepare spells from the Runebinder list.",
  "",
  "### Rune Etching (level 2)",
  "Etch a rune on a weapon as a bonus action.",
  "",
  "### Rune Ward (level 5)",
  "Reaction: gain resistance to one damage type until your next turn.",
  "",
  "### Glyph Mastery (level 11)",
  "Your etched runes also deal 1d6 force damage.",
  "",
  "### Master Runebinder (level 20)",
  "Once per day, rewrite the world: cast any spell without a slot.",
  "",
  "## Species: Rockling",
  "Rocklings are small, quiet folk who nap in sunlit quarries.",
  "Size: Small.",
  "Stone Skin. Your armor class is 12 plus your Dexterity modifier when unarmored.",
  "Quarry Sense. You have darkvision out to 60 feet.",
  "Pebble Drift. You have a flying speed of 30 feet.",
  "",
  "## Feat: Brisk Walker",
  "Your walking speed increases by 10 feet.",
  "",
  "## Magic Items",
  "| Item | Rarity | Attunement |",
  "|---|---|---|",
  "| Lantern of Quiet Hours | - | No |",
  "| Pebble Charm | Rare | Yes |",
  "| Cairn Staff | Very Rare | Yes |",
  "",
  "Lantern of Quiet Hours. Wondrous item. While lit, it sheds dim light in a 20-foot radius and sounds are softened.",
  "Pebble Charm. A smooth stone on a cord. Once per day you can cast Feather Fall from it.",
  "Cairn Staff. Stacked stones form its head.",
  "",
  "## Weapon property: Tumbling",
  "A weapon with Tumbling can be thrown and returns to your hand on a 20.",
  "",
  "## Closing notes",
  "All content above is invented for testing.",
];
const SUBCLASS_SOURCE = [
  "# Path of the Stormcaller (synthetic sample)",
  "Primal path of storm and thunder.",
  "",
  "### Storm Aura (level 3)",
  "Emanation of lightning while raging.",
];
const genericSource = (kind: ImportKind, name: string): string[] => [`# ${name} (synthetic sample)`, `${name} as written in the source.`, `Kind: ${kind}.`];

const OK_REVIEW: ImportElementReview = { state: "ok", code: "ok", reason: "", details: [], textOnly: false, textOnlyReason: "", textOnlyOverride: null, accepted: false };
const USER_TEXT_ONLY_REASON = "You chose text only.";

function review(patch: Partial<ImportElementReview> = {}): ImportElementReview {
  return { ...OK_REVIEW, ...patch };
}

function passageOf(lines: string[], startLine: number, endLine: number): ImportSourcePassage {
  return { startLine, endLine, text: lines.slice(startLine - 1, endLine).join("\n") };
}

function el(id: string, type: string, name: string, level: number | undefined, status: ImportElement["status"], excerpt: string, extra: Partial<ImportElement> = {}): ImportElement {
  return { id, type, name, ...(level ? { level } : {}), status, sourceExcerpt: excerpt, json: { id, type, name, ...(level ? { level } : {}), description: excerpt }, balanceFlags: [], validationIssues: [], review: OK_REVIEW, sourcePassage: null, ...extra };
}

export function countReviews(elements: ImportElement[]): ImportReviewCounts {
  const count = (f: (e: ImportElement) => boolean) => elements.filter(f).length;
  return {
    ok: count((e) => e.review.state === "ok"),
    check: count((e) => e.review.state === "check"),
    notImported: count((e) => e.review.state === "not_imported"),
    textOnly: count((e) => e.review.textOnly),
    accepted: count((e) => e.review.accepted),
  };
}

function runebinder(L: string[]): ImportElement[] {
  const at = (a: number, b: number) => passageOf(L, a, b);
  const chars = L.join("\n").length;
  return [
    el("runebinder", "class", "Runebinder", undefined, "ok", "Hit Die d8. Saving throws: Intelligence, Wisdom.", { sourcePassage: at(3, 4) }),
    el("runebinder-spellcasting", "spellcasting", "Runebinder Spellcasting", 1, "ok", "You know two cantrips and prepare spells from the Runebinder list.", { sourcePassage: at(5, 5) }),
    el("rune-etching", "class_feature", "Rune Etching", 2, "repaired", "Etch a rune on a weapon as a bonus action.", {
      validationIssues: ["Fixed: usage 'prof bonus per rest' was not a valid recharge; set to long rest."],
      sourcePassage: at(7, 8),
      review: review({ details: ["Fixed automatically: usage 'prof bonus per rest' was not a valid recharge; set to long rest."] }),
    }),
    el("rune-ward", "class_feature", "Rune Ward", 5, "ok", "Reaction: gain resistance to one damage type until your next turn.", { sourcePassage: at(10, 11) }),
    el("glyph-mastery", "class_feature", "Glyph Mastery", 11, "ok", "Your etched runes also deal 1d6 force damage.", { sourcePassage: at(13, 14) }),
    el("master-runebinder", "class_feature", "Master Runebinder", 20, "needs_review", "Once per day, rewrite the world: cast any spell without a slot.", {
      balanceFlags: ["Capstone has no cost; similar capstones are limited to once per long rest."],
      validationIssues: ["Spell level cap missing; assumed 9."],
      sourcePassage: at(16, 17),
      review: review({
        state: "check", code: "check:balance", reason: "The capstone has no cost; similar capstones are limited to once per long rest.",
        details: ["Spell level cap missing; assumed 9."],
      }),
    }),
    el("rockling", "species", "Rockling", undefined, "needs_review", "Rocklings are small, quiet folk who nap in sunlit quarries.", {
      sourcePassage: at(19, 24),
      review: review({
        state: "check", code: "mechanic_lost",
        reason: 'The source says "You have a flying speed of 30 feet" but this element has no rule or text for it',
        details: ["It has no speed: its source passage was probably cut short."],
      }),
    }),
    el("brisk-walker", "feat", "Brisk Walker", undefined, "ok", "Your walking speed increases by 10 feet.", {
      sourcePassage: at(26, 27),
      review: review({
        state: "ok", code: "accepted", reason: "You checked this and marked it as fine.", accepted: true,
        details: ["It has no prerequisite: its source passage was probably cut short. Point to the full passage or add the missing traits."],
      }),
    }),
    el("lantern-of-quiet-hours", "item", "Lantern of Quiet Hours", undefined, "ok", "While lit, it sheds dim light in a 20-foot radius and sounds are softened.", {
      sourcePassage: at(34, 34),
      review: review({ details: ["The source gives no rarity, so I assumed Common."] }),
    }),
    el("pebble-charm", "item", "Pebble Charm", undefined, "ok", "A smooth stone on a cord. Once per day you can cast Feather Fall from it.", {
      sourcePassage: at(35, 35),
      review: review({ textOnly: true, textOnlyReason: "The sheet does not track the daily use of this charm." }),
    }),
    el("cairn-staff", "item", "Cairn Staff", undefined, "needs_review", "Stacked stones form its head.", {
      json: {},
      validationIssues: ["Generation failed: the model returned no usable data."],
      review: review({ state: "not_imported", code: "failed", reason: "Not imported: generation failed: the model returned no usable data.", details: ["Try fixing it with AI, or point to the section it comes from."] }),
    }),
    el("tumbling", "weapon_property", "Tumbling", undefined, "ok", "A weapon with Tumbling can be thrown and returns to your hand on a 20.", {
      sourcePassage: at(39, 40),
      review: review({ textOnly: true, textOnlyReason: "A new weapon property: the app applies no rules for it." }),
    }),
    el("hearth-rite", "class_feature", "Hearth Rite", 9, "needs_review", "Spend ten minutes at a fire to restore a rune.", {
      review: review({
        state: "check", code: "unlocated",
        reason: `No section for "Hearth Rite" was found in the source; it was generated from all ${chars} characters`,
        details: ["Point to the lines it comes from so it can be checked against the source."],
      }),
    }),
  ];
}

function subclass(L: string[]): ImportElement[] {
  return [
    el("path-of-the-stormcaller", "subclass", "Path of the Stormcaller", 3, "ok", "Primal path of storm and thunder.", { sourcePassage: passageOf(L, 1, 2) }),
    el("storm-aura", "class_feature", "Storm Aura", 3, "ok", "Emanation of lightning while raging.", { sourcePassage: passageOf(L, 4, 5) }),
  ];
}

function generic(kind: ImportKind, name: string, L: string[]): ImportElement[] {
  return [el(`imported-${kind}`, kind, name, undefined, "ok", `${name} as written in the source.`, { sourcePassage: passageOf(L, 1, 2) })];
}

function sourceFor(kind: ImportKind, name: string): string[] {
  return kind === "class" ? CLASS_SOURCE : kind === "subclass" ? SUBCLASS_SOURCE : genericSource(kind, name);
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
      sourceLines: sourceFor(kind, detected.names[0]!), reviewCounts: countReviews([]), importNotes: [],
      ...(needsParent ? { parentChoice: { candidates: PARENTS } } : {}),
      ...(req.hint?.parentClassId ? { parentClassId: req.hint.parentClassId } : {}),
    };
    this.drafts.set(draftId, draft);
    void this.run(draftId, kind, detected.names[0]!);
    return { draftId };
  }

  private put(id: string, patch: Partial<ImportDraft>): ImportDraft {
    const merged = { ...this.drafts.get(id)!, ...patch };
    const next = patch.elements ? { ...merged, reviewCounts: countReviews(merged.elements) } : merged;
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
    const lines = this.drafts.get(draftId)!.sourceLines;
    const els = kind === "class" ? runebinder(lines) : kind === "subclass" ? subclass(lines) : generic(kind, name, lines);
    for (let i = 0; i < els.length; i++) {
      await sleep(this.stepMs);
      this.put(draftId, { elements: [...this.drafts.get(draftId)!.elements, els[i]!] });
      this.send(draftId, "generate", 0.3 + (0.4 * (i + 1)) / els.length, `Writing ${els[i]!.name}`, els[i]);
    }
    await sleep(this.stepMs);
    const levels = kind === "class" ? 20 : 0;
    this.send(draftId, "validate", 0.85, kind === "class" ? "Playing a test character from level 1 to 20" : "Checking against the rules engine");
    const notes = kind === "class" ? [{ code: "heading_uncovered", summary: "These headings weren't imported", items: ["Camp Rules", "Road Lore"] }] : [];
    this.put(draftId, { sweep: { levelsPassed: levels, errors: [] }, importNotes: notes });
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
      const fixed: ImportElement = { ...target, status: "ok", balanceFlags: [], validationIssues: [], review: review({ textOnly: target.review.textOnly, textOnlyReason: target.review.textOnlyReason, textOnlyOverride: target.review.textOnlyOverride }), json: { ...target.json, revision: instruction } };
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
    const elements = d.elements.map((e) => (e.id === elementId ? { ...e, json, name: typeof json.name === "string" && json.name ? json.name : e.name, validationIssues: issues, status: issues.length ? ("failed" as const) : ("ok" as const), balanceFlags: [], review: issues.length ? review({ state: "not_imported", code: "failed", reason: `Not imported: ${issues[0]}` }) : review({ textOnly: e.review.textOnly, textOnlyReason: e.review.textOnlyReason, textOnlyOverride: e.review.textOnlyOverride }) } : e));
    return this.put(draftId, { elements });
  }

  private replaceElement(draftId: string, elementId: string, change: (e: ImportElement) => ImportElement): ImportDraft {
    const d = this.get(draftId);
    if (!d.elements.some((e) => e.id === elementId)) throw apiError("not_found", "Unknown element.");
    return this.put(draftId, { elements: d.elements.map((e) => (e.id === elementId ? change(e) : e)) });
  }

  acceptElement(draftId: string, elementId: string, accepted: boolean): ImportDraft {
    return this.replaceElement(draftId, elementId, (e) => {
      const r = e.review;
      if (r.state === "not_imported") throw apiError("invalid_argument", "This element was not imported, so there is nothing to accept.");
      if (accepted && r.state === "check") return { ...e, review: { ...r, state: "ok", code: "accepted", reason: "You checked this and marked it as fine.", details: [r.reason, ...r.details], accepted: true } };
      if (!accepted && r.accepted) {
        const [original = "", ...rest] = r.details;
        return { ...e, review: { ...r, state: "check", code: "check", reason: original, details: rest, accepted: false } };
      }
      return e;
    });
  }

  setTextOnly(draftId: string, elementId: string, textOnly: boolean | null): ImportDraft {
    return this.replaceElement(draftId, elementId, (e) => {
      const r = e.review;
      const automatic = r.textOnlyReason !== "" && r.textOnlyReason !== USER_TEXT_ONLY_REASON;
      const reason = textOnly === null ? (automatic ? r.textOnlyReason : "") : textOnly ? (r.textOnlyReason || USER_TEXT_ONLY_REASON) : r.textOnlyReason;
      const next: ImportElementReview = { ...r, textOnlyOverride: textOnly, textOnly: textOnly ?? automatic, textOnlyReason: reason };
      if (textOnly !== null && r.state === "check" && r.code === "mechanic_lost") {
        return { ...e, review: { ...next, state: "ok", code: "ok", reason: "", details: [r.reason, ...r.details] } };
      }
      return { ...e, review: next };
    });
  }

  async relocate(draftId: string, elementId: string, startLine: number, endLine: number): Promise<void> {
    const d = this.get(draftId);
    const target = d.elements.find((e) => e.id === elementId);
    if (!target) throw apiError("not_found", "Unknown element.");
    if (!Number.isInteger(startLine) || !Number.isInteger(endLine) || startLine < 1 || startLine > endLine || endLine > d.sourceLines.length) {
      throw apiError("invalid_argument", `Pick lines between 1 and ${d.sourceLines.length}, with the first line before the last.`);
    }
    const sourcePassage = passageOf(d.sourceLines, startLine, endLine);
    void (async () => {
      await sleep(this.stepMs);
      this.send(draftId, "repair", 0.5, `Reading lines ${startLine} to ${endLine} for ${target.name}`);
      await sleep(this.stepMs);
      const fixed: ImportElement = {
        ...target, status: "ok", balanceFlags: [], validationIssues: [], sourcePassage, sourceExcerpt: sourcePassage.text,
        review: review({ textOnly: target.review.textOnly, textOnlyReason: target.review.textOnlyReason, textOnlyOverride: target.review.textOnlyOverride }),
      };
      this.put(draftId, { elements: this.get(draftId).elements.map((e) => (e.id === target.id ? fixed : e)) });
      this.send(draftId, "repair", 0.8, `Updated ${target.name}`, fixed);
      await sleep(this.stepMs);
      this.send(draftId, "ready", 1, "Ready for your review");
    })();
  }

  /** Generate the element again from its current passage (mock: same result as relocating onto the lines it already has). */
  async regenerate(draftId: string, elementId: string): Promise<void> {
    const target = this.get(draftId).elements.find((e) => e.id === elementId);
    if (!target) throw apiError("not_found", "Unknown element.");
    const p = target.sourcePassage;
    if (!p) throw apiError("invalid_argument", "This element has no source passage yet. Pick one in the source first.");
    return this.relocate(draftId, elementId, p.startLine, p.endLine);
  }

  finalize(draftId: string, packName: string, prefix: string): { name: string; count: number; prefix: string } {
    const d = this.get(draftId);
    if (d.parentChoice) throw apiError("invalid_argument", "Choose the parent class first.");
    if (d.elements.some((e) => e.status === "failed" && e.review.state !== "not_imported")) throw apiError("content_error", "Fix the failed elements first.");
    if (!packName.trim() || !/^[a-z0-9-]{1,8}$/.test(prefix)) throw apiError("invalid_argument", "Pack name and a short lowercase prefix are required.");
    this.drafts.delete(draftId);
    return { name: packName.trim(), count: d.elements.filter((e) => e.review.state !== "not_imported").length, prefix };
  }

  discard(draftId: string): void { this.drafts.delete(draftId); }
}
