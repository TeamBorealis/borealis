import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { seededId } from "./utils";

// The mock host gives the Play tab's roll values as the real host does (src-tauri/tests/play_rolls.rs): ability checks,
// attack to-hit, the spell attack roll and the 2024 Exhaustion penalty in each, ready spells, hit dice and resource dice.

const api = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

describe("mock roll values", () => {
  it("a 2024 character's checks, to-hit and spell attack roll lose 2 per exhaustion level", async () => {
    // Arrange
    const m = api();
    const id = await seededId(m, "Seraphine");
    const before = await m.getSnapshot({ characterId: id });

    // Act
    const after = await m.setExhaustion({ characterId: id, level: 1 });

    // Assert
    expect(before.abilities.check).toEqual(before.abilities.modifier);
    expect(after.abilities.check.dex).toBe(before.abilities.modifier.dex - 2);
    expect(before.attacks.find((a) => a.name === "Fire Bolt")?.toHit).toBe(5);
    expect(after.attacks.find((a) => a.name === "Fire Bolt")?.toHit).toBe(3);
    const caster = after.spellcasting.classes[0]!;
    expect(caster.attackRoll).toBe(caster.attackBonus - 2);
  });

  it("only prepared spells and cantrips are ready, and each class has its hit dice", async () => {
    const m = api();
    const s = await m.getSnapshot({ characterId: await seededId(m, "Seraphine") });
    const ready = s.spellcasting.classes[0]!.spells.filter((x) => x.ready).map((x) => x.name);
    expect(ready).toEqual(expect.arrayContaining(["Fire Bolt", "Magic Missile", "Shield"]));
    expect(ready).not.toContain("Sleep");
    expect(s.play.hitDice).toEqual([{ className: "Wizard", die: 6, total: 2, notation: `1d6+${s.abilities.modifier.con}` }]);
    expect(s.abilities.modifier.con).toBe(2);
    expect(s.play.resources.find((r) => r.name === "Inspiration Die")?.die).toBe("1d6");
    expect(s.play.resources.find((r) => r.name === "Rage")?.die).toBeNull();
  });

  it("a 2014 character's longsword row carries its to-hit and a save DC has none", async () => {
    const m = api();
    const s = await m.getSnapshot({ characterId: await seededId(m, "Brannoc") });
    const sword = s.attacks.find((a) => a.name === "Longsword")!;
    expect(sword.toHit).toBe(Number(sword.attackBonus));
    expect(sword.damage).toMatch(/^1d8\+\d slashing$/);
    const saved = await m.upsertAttack({ characterId: s.id, attack: { ...sword, id: undefined, name: "Breath", auto: false, attackBonus: "DC 13 DEX" } });
    expect(saved.attacks.find((a) => a.name === "Breath")?.toHit).toBeNull();
  });
});
