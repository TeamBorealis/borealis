import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";

// IDprw: the web mock's Try it gives a magic weapon on one base item that base's attack line, its bonus added to the
// attack and damage rolls, as the backend does (`homebrew_p2b.rs`
// `item_drafts_tried_out_are_equipped_with_their_attack_line_and_armor_class`: a +1 weapon on a Longsword is held,
// one better to hit, 1d8 + Strength + 1). One that names a group of bases has no base to stand for, on both.
const specific = (id: string) => ({ value: { shape: "model", model: { specific: true, root: { node: "row", row: { row: "element", id } } } } });

async function tryIt(fields: Record<string, unknown>) {
  const api = new MockApi({ creatorEnabled: true });
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const fresh = await api.homebrewNewDraft({ packId: pack.id, kind: "magicItem" });
  const draft = await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, ...fields } } });
  return api.homebrewTrialStart({ draftId: draft.draftId, setup: { level: 1 } });
}

describe("the mock's Try it with a magic weapon", () => {
  it("holds a +1 weapon on one base and gives its attack line the +1", async () => {
    const view = await tryIt({ name: "Emberblade", magicType: "Weapon", rarity: "Uncommon", appliesToWeapon: specific("longsword"), enhancement: 1 });
    const s = view.snapshot;
    expect(s.inventory.items.find((i) => i.name === "Emberblade")).toMatchObject({ equipped: "primary", isWeapon: true });
    const str = s.abilities.modifier.str;
    const line = s.attacks.find((a) => a.name === "Emberblade");
    expect(line).toMatchObject({ toHit: s.stats.proficiencyBonus + str + 1, damage: `1d8+${str + 1} slashing` });
    expect(view.changed).toContain("attacks:Emberblade");
  });

  it("gives one for a group of bases no attack line", async () => {
    const martial = { value: { shape: "model", model: { specific: false, root: { node: "row", row: { row: "tag", tag: "Martial" } } } } };
    const view = await tryIt({ name: "Emberbrand", magicType: "Weapon", rarity: "Uncommon", appliesToWeapon: martial, enhancement: 1 });
    expect(view.snapshot.attacks.some((a) => a.name === "Emberbrand")).toBe(false);
  });
});
