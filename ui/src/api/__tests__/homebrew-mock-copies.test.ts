import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";

// H10ac: the web mock's weapon and armor templates and copies are the real backend's (`homebrew-copies.ts`, written by
// `src-tauri/tests/homebrew_copy.rs` `copies_match_the_mock`): "Start from a template" finds the SRD weapons, "New from
// copy" of a sample weapon carries the real form of the SRD weapon of its name, and DATA shows the element the
// backend wrote for that copy, under the draft's own id, name and book.
async function pack(api: MockApi) {
  return api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
}

describe("the web mock's weapon and armor copies", () => {
  it("offers the SRD weapons and armor as templates", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const names = async (types: string[]) => (await api.homebrewReferenceOptions({ target: { types }, query: "", filters: { srdOnly: true }, page: { offset: 0, limit: 50 } }))
      .items.map((r) => r.name);
    expect(await names(["Weapon"])).toEqual(["Dagger", "Greatsword", "Handaxe", "Longsword", "Mace", "Rapier", "Shortbow", "Spear"]);
    expect(await names(["Armor"])).toEqual(["Chain Mail", "Chain Shirt", "Leather Armor", "Plate Armor", "Shield"]);
  });

  it("copies a sample weapon with every field of the SRD weapon of its name", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { id: packId } = await pack(api);
    const copy = await api.homebrewNewDraft({ packId, kind: "weapon", copyFrom: "longsword" });
    expect(copy.form.fields).toMatchObject({
      name: "Longsword (copy)", source: "Ember Isles", damage: "1d8", versatile: "1d10", cost: "15", currency: "gp", weight: "3",
      damageType: { name: "Slashing" }, weaponCategory: { name: "Martial Melee" }, mastery: [{ name: "Sap" }], properties: [{ name: "Versatile" }],
    });
    const data = JSON.parse(copy.generated.json) as Record<string, unknown>;
    expect(data).toMatchObject({ id: "ID_EMB_WEAPON_LONGSWORD_COPY", name: "Longsword (copy)", type: "Weapon", source: "Ember Isles" });
    expect(data.setters).toEqual(expect.arrayContaining([{ name: "damage", value: "1d8", attributes: { type: "slashing" } }]));
    expect(copy.generated.tree[0]).toMatchObject({ key: "Longsword (copy)" });
    expect(copy.generated.tree[0]!.children!.find((n) => n.key === "id")).toMatchObject({ text: "ID_EMB_WEAPON_LONGSWORD_COPY" });

    // renamed, its DATA follows
    const renamed = await api.homebrewUpdateDraft({ draftId: copy.draftId, form: { ...copy.form, fields: { ...copy.form.fields, name: "Ember Sword" } } });
    expect(JSON.parse(renamed.generated.json)).toMatchObject({ id: "ID_EMB_WEAPON_EMBER_SWORD", name: "Ember Sword" });
  });

  it("copies a sample armor from the SRD armor of its name", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { id: packId } = await pack(api);
    const copy = await api.homebrewNewDraft({ packId, kind: "armor", copyFrom: "chain-mail" });
    expect(copy.form.fields).toMatchObject({ name: "Chain Mail (copy)", armorType: "Heavy", baseAc: 16, stealth: true, strength: 13 });
  });
});
