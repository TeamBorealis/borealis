import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { COMMANDS } from "../contract";

const PACK = { name: "Gloom", code: "GL", edition: "dnd2024" as const };

describe("homebrew creator contract (WB-00)", () => {
  it("names every creator command after its method", () => {
    const creator = Object.entries(COMMANDS).filter(([m]) => m.startsWith("homebrew"));
    // P2a added homebrew_child_use
    expect(creator).toHaveLength(27);
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
    // a new spell starts with the spell form's values (WB-11); its school is the author's to choose
    expect(draft.form.fields).toMatchObject({ level: "1", time: "Action", range: "Self", duration: "Instantaneous", verbal: true, somatic: true, material: false });
    expect(draft.problems.map((p) => p.path)).toEqual(["fields.name", "fields.school"]);
    const named = await api.homebrewUpdateDraft({ draftId: draft.draftId, form: { ...draft.form, fields: { ...draft.form.fields, name: "Ember Bolt", school: "Evocation" } } });
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

  it("exports a pack file and imports it back as a copy with new ids (WB-24, spec 14.2)", async () => {
    // Arrange
    const api = new MockApi();
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const d = await api.homebrewNewDraft({ packId, kind: "feat" });
    await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { name: "Sturdy" } } });
    await api.homebrewSaveDraft({ draftId: d.draftId });

    // Act
    const file = await api.homebrewExportPack({ packId, leaveOutCopiedText: true });
    const preview = await api.previewImportPack({ request: { files: [{ fileName: file.fileName, contentBase64: file.dataBase64! }] } });
    const copy = await api.installImportPreview({ previewId: preview.previewId, asCopy: true });

    // Assert
    expect(file).toMatchObject({ fileName: "Gloom.borealis-pack.zip", path: null, shared: false, leftOut: 0 });
    expect(preview).toMatchObject({ packId, replacesExisting: true, converted: 1, homebrew: { code: "GL", codeTaken: false, copyCode: "GL2", copyName: "Gloom (copy)" } });
    expect(copy.pack).toMatchObject({ id: "homebrew-gl2", name: "Gloom (copy)", kind: "homebrew" });
    expect((await api.homebrewListElements({ packId: "homebrew-gl2" })).map((r) => r.id)).toEqual(["ID_GL2_FEAT_STURDY"]);
    expect((await api.homebrewListElements({ packId })).map((r) => r.id)).toEqual(["ID_GL_FEAT_STURDY"]);
  });

  it("serves the block and statistic catalogs the backend sends (WB-05)", async () => {
    const api = new MockApi();
    const blocks = await api.homebrewBlockCatalog();
    expect(blocks.slice(0, 4).map((b) => b.kind)).toEqual(["grant", "select", "list", "stat"]);
    expect(blocks.filter((b) => !["basic", "effects"].includes(b.group))).toHaveLength(69);
    const resource = blocks.find((b) => b.kind === "resource");
    expect(resource).toMatchObject({ group: "uses", common: true, levelGated: true, onlyIf: true });
    const max = resource?.fields.find((f) => f.key === "max");
    expect(max?.variants?.map((v) => v.value)).toEqual(["fixed", "stat", "table", "formula", "rolled"]);
    expect(max?.variants?.find((v) => v.value === "formula")?.fields[0]).toMatchObject({ dialect: "formula", bindings: ["max.expr"] });
    const stats = await api.homebrewStatCatalog();
    expect(stats[0]?.id).toBe("abilities");
    expect(stats.flatMap((g) => g.stats).find((s) => s.name === "darkvision:range")?.label).toBe("Darkvision range");
  });
});
