import { describe, expect, it, vi } from "vitest";
import type { HomebrewTrialChange, HomebrewTrialView } from "../api/contract";
import { MockApi } from "../api/mock";
import { standardArrayFor } from "../api/mock/homebrew-trial";
import { placementText, takenAtLevel, trialSlotOf } from "../pages/homebrew/tryit/placement";
import { TRIAL_ONLY_CHOICES, trialApi } from "../pages/homebrew/tryit/trialScope";
import { type TrialApi, TrialSession, mergeChanges } from "../pages/homebrew/tryit/trialSession";
import { slotsText } from "../pages/homebrew/tryit/TrialLadder";
import { swapScores } from "../pages/homebrew/tryit/TrialSetup";
import { changedSet } from "../pages/homebrew/tryit/TrialSummary";

// Homebrew WB-21 (spec 9): the mock's trial character (the stand-in for WB-08), the Try it session (one update in flight,
// merged level steps, late answers, an expired trial) and the helpers of the flyout.

const PACK = { name: "Gloom", code: "GL", edition: "dnd2024" as const };

/** A saved-in-draft feat "Fleet" whose rule adds 5 to walking speed. */
async function featDraft(api: MockApi, fields: Record<string, unknown> = {}) {
  const { id: packId } = await api.homebrewCreatePack({ request: PACK });
  const d = await api.homebrewNewDraft({ packId, kind: "feat" });
  const view = await api.homebrewUpdateDraft({
    draftId: d.draftId,
    form: { ...d.form, fields: { name: "Fleet", category: "General", ...fields }, blocks: [{ key: "b1", kind: "stat", sentence: "", fields: { name: "speed", value: "5" } }] },
  });
  return { packId, draftId: view.draftId };
}

const view = (id: string, level = 1): HomebrewTrialView => ({
  trialId: id, snapshot: { level: { total: level } } as HomebrewTrialView["snapshot"], ladder: [], problems: [], changed: [],
});

/** A backend whose answers wait until the test lets them go. */
function gatedApi() {
  const calls: { kind: "start" | "update" | "end"; arg: unknown; release: (v?: HomebrewTrialView, err?: unknown) => void }[] = [];
  const pending = (kind: "start" | "update" | "end", arg: unknown) => new Promise<HomebrewTrialView>((resolve, reject) => {
    calls.push({ kind, arg, release: (v, err) => (err ? reject(err) : resolve(v!)) });
  });
  const api: TrialApi = {
    homebrewTrialStart: (a) => pending("start", a),
    homebrewTrialUpdate: (a) => pending("update", a),
    homebrewTrialEnd: vi.fn(async () => undefined),
  };
  return { api, calls };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("WB-21 mock trial (stand-in for WB-08)", () => {
  it("starts a level 1 test character with the edition's SRD picks and a 1-20 ladder; the feat waits for level 4", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api);
    const before = await api.listCharacters();
    const v = await api.homebrewTrialStart({ draftId, setup: {} });
    expect(v.snapshot.level.total).toBe(1);
    expect(v.snapshot.ruleset).toBe("dnd2024");
    expect(v.snapshot.level.classes[0]?.name).toBe("Fighter");
    expect(v.ladder.map((r) => r.level)).toEqual(Array.from({ length: 20 }, (_, i) => i + 1));
    expect(v.ladder[3]?.features).toContain("Fleet");
    expect(v.snapshot.play.features.some((f) => f.name === "Fleet")).toBe(false);
    expect(v.changed).toEqual([]);
    // Never listed with the user's characters (spec 9).
    expect(await api.listCharacters()).toEqual(before);
  });

  it("levels the test character: the feat arrives at 4 with its speed, both marked as changed", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api);
    const start = await api.homebrewTrialStart({ draftId, setup: {} });
    const v = await api.homebrewTrialUpdate({ trialId: start.trialId, change: { level: 4 } });
    expect(v.snapshot.level.total).toBe(4);
    expect(v.snapshot.play.features.find((f) => f.name === "Fleet")?.level).toBe(4);
    expect(v.snapshot.stats.speed).toBe(start.snapshot.stats.speed + 5);
    expect(v.changed).toEqual(["play.features:Fleet", "stats.speed"]);
    expect(v.ladder[2]?.choices).toContain("Fighter subclass");
  });

  it("an origin feat of 2024 is taken at level 1", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api, { category: "Origin" });
    const v = await api.homebrewTrialStart({ draftId, setup: {} });
    expect(v.snapshot.play.features.find((f) => f.name === "Fleet")?.level).toBe(1);
    expect(v.ladder[0]?.features).toContain("Fleet");
  });

  it("lists a choice's options, takes a pick, clears it, and refuses one the slot does not offer", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api);
    const v = await api.homebrewTrialStart({ draftId, setup: {} });
    const skill = v.snapshot.slots.find((s) => s.elementType === "skill" && !s.chosen)!;
    const page = await api.homebrewTrialOptions({ trialId: v.trialId, slotId: skill.id });
    expect(page.items.map((o) => o.name)).toContain("Athletics");
    const athletics = page.items.find((o) => o.name === "Athletics")!;
    const picked = await api.homebrewTrialUpdate({ trialId: v.trialId, change: { slotId: skill.id, elementId: athletics.id } });
    expect(picked.snapshot.slots.find((s) => s.id === skill.id)?.chosen?.name).toBe("Athletics");
    const cleared = await api.homebrewTrialUpdate({ trialId: v.trialId, change: { slotId: skill.id, elementId: null } });
    expect(cleared.snapshot.slots.find((s) => s.id === skill.id)?.chosen).toBeNull();
    await expect(api.homebrewTrialUpdate({ trialId: v.trialId, change: { slotId: skill.id, elementId: "fireball" } })).rejects.toMatchObject({ code: "requirement_failed" });
  });

  it("changes edition (back to that edition's picks) and ability scores; refuses bad scores; ends", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api);
    const v = await api.homebrewTrialStart({ draftId, setup: { level: 3 } });
    const other = await api.homebrewTrialUpdate({ trialId: v.trialId, change: { setup: { edition: "dnd2014" } } });
    expect(other.snapshot.ruleset).toBe("dnd2014");
    expect(other.snapshot.level.total).toBe(3);
    const swapped = await api.homebrewTrialUpdate({ trialId: v.trialId, change: { setup: { abilityScores: [8, 14, 13, 12, 10, 15] } } });
    expect(swapped.snapshot.abilities.base).toMatchObject({ str: 8, cha: 15 });
    await expect(api.homebrewTrialUpdate({ trialId: v.trialId, change: { setup: { abilityScores: [1, 2] } } })).rejects.toMatchObject({ code: "invalid_argument" });
    await api.homebrewTrialEnd({ trialId: v.trialId });
    await expect(api.homebrewTrialUpdate({ trialId: v.trialId, change: { level: 2 } })).rejects.toMatchObject({ code: "not_found" });
  });

  it("the standard array follows the class's main abilities", () => {
    expect(standardArrayFor("fighter")).toEqual([15, 14, 13, 10, 12, 8]);
    expect(standardArrayFor("wizard")[3]).toBe(15);
  });
});

describe("WB-21 TrialSession", () => {
  it("sends one update at a time and merges the level steps that wait", async () => {
    const { api, calls } = gatedApi();
    const s = new TrialSession(api, "d1");
    void s.start();
    calls[0]!.release(view("t1"));
    await tick();
    const a = s.change({ level: 2 });
    const b = s.change({ level: 3 });
    const c = s.change({ setup: { edition: "dnd2014" }, level: 4 });
    await tick();
    expect(calls.filter((x) => x.kind === "update").map((x) => (x.arg as { change: HomebrewTrialChange }).change)).toEqual([{ level: 2 }]);
    calls[1]!.release(view("t1", 2));
    await tick();
    expect((calls[2]!.arg as { change: HomebrewTrialChange }).change).toEqual({ level: 4, setup: { edition: "dnd2014" } });
    calls[2]!.release(view("t1", 4));
    expect((await a).snapshot.level.total).toBe(2);
    expect((await b).snapshot.level.total).toBe(4);
    expect((await c).snapshot.level.total).toBe(4);
    expect(s.getSnapshot()).toMatchObject({ busy: false, error: null });
    expect(s.getSnapshot().view?.snapshot.level.total).toBe(4);
  });

  it("a choice never merges with a level step", () => {
    expect(mergeChanges({ level: 2 }, { setup: { abilityScores: [1, 2, 3, 4, 5, 6] } })).toEqual({ level: 2, setup: { abilityScores: [1, 2, 3, 4, 5, 6] } });
  });

  it("keeps the last view and shows the error when an update is refused", async () => {
    const { api, calls } = gatedApi();
    const s = new TrialSession(api, "d1");
    void s.start();
    calls[0]!.release(view("t1"));
    await tick();
    const p = s.change({ slotId: "x", elementId: "y" });
    await tick();
    calls[1]!.release(undefined, { code: "requirement_failed", message: "That option is not available for this slot." });
    await expect(p).rejects.toMatchObject({ code: "requirement_failed" });
    expect(s.getSnapshot()).toMatchObject({ error: "That option is not available for this slot.", busy: false });
    expect(s.getSnapshot().view?.trialId).toBe("t1");
  });

  it("ends a trial whose start answers after the close", async () => {
    const { api, calls } = gatedApi();
    const s = new TrialSession(api, "d1");
    void s.start();
    await s.end();
    expect(api.homebrewTrialEnd).not.toHaveBeenCalled();
    calls[0]!.release(view("late"));
    await tick();
    expect(api.homebrewTrialEnd).toHaveBeenCalledWith({ trialId: "late" });
    expect(s.getSnapshot().view).toBeNull();
  });

  it("makes an expired trial again with the same setup and level, then sends the choice again", async () => {
    const { api, calls } = gatedApi();
    const s = new TrialSession(api, "d1");
    void s.start({ edition: "dnd2014" });
    calls[0]!.release(view("t1"));
    await tick();
    void s.change({ level: 5 });
    await tick();
    calls[1]!.release(view("t1", 5));
    await tick();
    const pick = s.change({ slotId: "s", elementId: "e" });
    await tick();
    calls[2]!.release(undefined, { code: "not_found", message: "gone" });
    await tick();
    expect(calls[3]).toMatchObject({ kind: "start", arg: { draftId: "d1", setup: { edition: "dnd2014", level: 5 } } });
    calls[3]!.release(view("t2", 5));
    await tick();
    expect(calls[4]).toMatchObject({ kind: "update", arg: { trialId: "t2", change: { slotId: "s", elementId: "e" } } });
    calls[4]!.release(view("t2", 5));
    expect((await pick).trialId).toBe("t2");
    expect(s.getSnapshot().notice).toMatch(/made again/);
  });

  it("refuses changes after the close and ends the trial once", async () => {
    const { api, calls } = gatedApi();
    const s = new TrialSession(api, "d1");
    void s.start();
    calls[0]!.release(view("t1"));
    await tick();
    await s.end();
    await s.end();
    expect(api.homebrewTrialEnd).toHaveBeenCalledTimes(1);
    await expect(s.change({ level: 2 })).rejects.toThrow(/closed/);
  });
});

describe("WB-21 helpers", () => {
  it("the trial Api routes choices to the trial and refuses every other character command", async () => {
    const api = new MockApi();
    const { draftId } = await featDraft(api);
    const s = new TrialSession(api, draftId);
    await s.start();
    const trialId = s.getSnapshot().view!.trialId;
    const routed = trialApi(api, s);
    const skill = s.getSnapshot().view!.snapshot.slots.find((x) => x.elementType === "skill")!;
    const options = await routed.getSlotOptions({ characterId: trialId, slotId: skill.id });
    const snap = await routed.applySelection({ characterId: trialId, slotId: skill.id, elementId: options.items[0]!.id });
    expect(snap.slots.find((x) => x.id === skill.id)?.chosen?.id).toBe(options.items[0]!.id);
    await expect(routed.levelUp({ characterId: trialId, classSlotId: "c0" })).rejects.toMatchObject({ message: TRIAL_ONLY_CHOICES });
    await expect(routed.saveCharacter({ characterId: trialId })).rejects.toMatchObject({ code: "invalid_argument" });
    expect((await routed.getElement({ elementId: "fighter" })).name).toBe("Fighter");
    await s.end();
  });

  it("names the draft's slot by its element type, and words its place", () => {
    expect(trialSlotOf({ elementTypes: ["Feat"] })).toBe("feat");
    expect(trialSlotOf({ elementTypes: ["Archetype", "Archetype Feature"] })).toBe("subclass");
    expect(trialSlotOf({ elementTypes: ["Race"] })).toBe("species");
    expect(trialSlotOf({ elementTypes: ["Magic Item"] })).toBe("item");
    expect(trialSlotOf({ elementTypes: [] })).toBe("grant");
    expect(placementText("class", true)).toMatch(/Fighter/);
    expect(placementText("feat", false)).toMatch(/first feat slot/);
  });

  it("reads the level a feature was taken at, swaps two scores, words slots and matches changes", () => {
    const snap = { play: { features: [{ name: "Fleet", level: 4 }] } } as unknown as HomebrewTrialView["snapshot"];
    expect(takenAtLevel(snap, " fleet ")).toBe(4);
    expect(takenAtLevel(snap, "Other")).toBeNull();
    expect(swapScores([15, 14, 13, 12, 10, 8], 0, 5)).toEqual([8, 14, 13, 12, 10, 15]);
    expect(slotsText([4, 3, 2, 0, 0, 0, 0, 0, 0])).toBe("4 / 3 / 2");
    expect(slotsText([0, 0, 0, 0, 0, 0, 0, 0, 0])).toBe("");
    const changed = changedSet(["stats.speed", "play.features:Fleet"]);
    expect(changed("stats.speed")).toBe(true);
    expect(changed("play.features:fleet")).toBe(true);
    expect(changed("stats.armorClass")).toBe(false);
  });
});
