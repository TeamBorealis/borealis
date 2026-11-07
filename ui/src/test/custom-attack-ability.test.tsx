import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { calculatedAttack, withModifier } from "../lib/attackCalc";
import { renderApp, seededId } from "./utils";

describe("attack calculation helpers (host borealis_character::with_modifier)", () => {
  it("puts the modifier after the leading dice and replaces one already there", () => {
    expect(withModifier("1d6 piercing", 3)).toBe("1d6+3 piercing");
    expect(withModifier("2d6 + 1 fire", 4)).toBe("2d6+4 fire");
    expect(withModifier("1d8-1", -2)).toBe("1d8-2");
    expect(withModifier("1d4+2 bludgeoning", 0)).toBe("1d4 bludgeoning");
    expect(withModifier("see notes", 3)).toBe("see notes");
  });

  it("adds proficiency to the attack bonus only", () => {
    const mods = { str: 3, dex: 1, con: 2, int: 0, wis: -1, cha: 4 };
    expect(calculatedAttack("1d6 piercing", "cha", mods, 3)).toEqual({ attackBonus: "+7", damage: "1d6+4 piercing" });
  });
});

describe("a custom attack worked out from an ability (GitHub #29)", () => {
  it("shows the worked-out bonus and damage, follows the picked ability and saves it", async () => {
    // Arrange: Brannoc, fighter 3 (proficiency +2), STR 16, DEX 12
    const api = new MockApi();
    const id = await seededId(api, "Brannoc");
    const user = userEvent.setup();
    renderApp("/manage", { api, openCharacterId: id });
    await user.click(await screen.findByRole("tab", { name: "Attacks & Spellcasting" }));
    await user.click(await screen.findByRole("button", { name: "Add Attack" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "Bite");
    const damage = within(dialog).getByLabelText(/^Damage/);
    await user.clear(damage);
    await user.type(damage, "1d6 piercing");

    // Act
    await user.click(within(dialog).getByLabelText("Calculate automatically"));

    // Assert
    expect(within(dialog).queryByLabelText("Attack bonus")).toBeNull();
    expect(within(dialog).getByText("Calculated with Strength: +5")).toBeTruthy();
    expect(within(dialog).getByText("Calculated with Strength: 1d6+3 piercing")).toBeTruthy();
    await user.selectOptions(within(dialog).getByLabelText("Ability (automatic calculation)"), "dex");
    expect(within(dialog).getByText("Calculated with Dexterity: +3")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(async () => {
      const bite = (await api.getSnapshot({ characterId: id })).attacks.find((a) => a.name === "Bite")!;
      expect(bite).toMatchObject({ calculated: true, ability: "dex", attackBonus: "+3", damage: "1d6+1 piercing" });
    });
  });
});
