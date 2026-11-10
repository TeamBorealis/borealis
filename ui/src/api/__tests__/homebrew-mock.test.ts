import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { COMMANDS } from "../contract";

const PACK = { name: "Gloom", code: "GL", edition: "dnd2024" as const };

describe("homebrew creator contract (WB-00)", () => {
  it("names every creator command after its method", () => {
    const creator = Object.entries(COMMANDS).filter(([m]) => m.startsWith("homebrew"));
    expect(creator).toHaveLength(24);
    for (const [method, command] of creator) expect(command).toBe(method.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`));
  });

  it("keeps the creator switched off unless asked (HC3)", async () => {
    expect((await new MockApi().getSettings()).creatorEnabled).toBe(false);
    expect((await new MockApi({ creatorEnabled: true }).getSettings()).creatorEnabled).toBe(true);
  });

  it("creates a pack that lists with the user's packs", async () => {
    const api = new MockApi();
    const pack = await api.homebrewCreatePack({ request: PACK });
    expect(pack).toMatchObject({ kind: "homebrew", name: "Gloom", rulesets: ["dnd2024"], elementCount: 0 });
    expect((await api.listPacks()).some((p) => p.id === pack.id)).toBe(true);
    await expect(api.homebrewCreatePack({ request: PACK })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.homebrewCreatePack({ request: { ...PACK, code: "g" } })).rejects.toMatchObject({ code: "invalid_argument" });
  });

  it("runs a draft from new to saved, blocking Save while a name is missing", async () => {
    const api = new MockApi();
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const draft = await api.homebrewNewDraft({ packId, kind: "spell" });
    expect(draft.problems[0]).toMatchObject({ severity: "error", path: "fields.name" });
    await expect(api.homebrewSaveDraft({ draftId: draft.draftId })).rejects.toMatchObject({ code: "invalid_argument" });
    const named = await api.homebrewUpdateDraft({ draftId: draft.draftId, form: { ...draft.form, fields: { name: "Ember Bolt", level: 1 } } });
    expect(named.problems).toEqual([]);
    expect((await api.homebrewListDrafts()).map((d) => d.name)).toEqual(["Ember Bolt"]);
    const saved = await api.homebrewSaveDraft({ draftId: draft.draftId });
    expect(saved.elementIds).toEqual(["ID_GL_SPELL_EMBER_BOLT"]);
    expect(saved.pack).toMatchObject({ elementCount: 1, version: "1.0.1" });
    expect(await api.homebrewListDrafts()).toEqual([]);
    const [row] = await api.homebrewListElements({ packId });
    expect(row).toMatchObject({ name: "Ember Bolt", kind: "spell", elementType: "Spell", hasDraft: false });
  });

  it("duplicates and deletes elements", async () => {
    const api = new MockApi();
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const d = await api.homebrewNewDraft({ packId, kind: "feat" });
    await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { name: "Sturdy" } } });
    const [id] = (await api.homebrewSaveDraft({ draftId: d.draftId })).elementIds;
    const copy = await api.homebrewDuplicateElement({ packId, elementId: id! });
    expect(copy.form.fields.name).toBe("Sturdy (copy)");
    expect(copy.elementId).toBeNull();
    const del = await api.homebrewDeleteElements({ packId, elementIds: [id!], confirm: false });
    expect(del).toMatchObject({ deleted: true, usedBy: [] });
    expect(await api.homebrewListElements({ packId })).toEqual([]);
  });

  it("answers not implemented where the backend has no work item yet", async () => {
    const api = new MockApi();
    await expect(api.homebrewTrialEnd({ trialId: "t" })).rejects.toMatchObject({
      code: "requirement_failed",
      details: { notImplemented: "homebrew_trial_end" },
    });
  });
});
