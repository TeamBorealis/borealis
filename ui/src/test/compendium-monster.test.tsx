import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "./utils";

async function openCompendium(query: string) {
  const user = userEvent.setup();
  renderApp("/characters");
  await user.type(await screen.findByLabelText("Compendium search"), `${query}{Enter}`);
  return { user, dlg: await screen.findByRole("dialog", { name: "Compendium" }) };
}

describe("compendium: monster entry (B-05)", () => {
  it("monsters have their own Monster group and type filter with a count", async () => {
    const { dlg } = await openCompendium("unicorn");
    expect(await within(dlg).findByRole("list", { name: "Compendium results: Monster" })).toBeTruthy();
    const filter = within(dlg).getByRole("combobox", { name: "Filter by type" });
    expect(within(filter).getByRole("option", { name: "Monster (2)" })).toBeTruthy();
  });

  it("the entry reads name, Monster type line, the whole stat block, then traits, actions and legendary actions", async () => {
    const { user, dlg } = await openCompendium("unicorn");
    await user.click(await within(dlg).findByRole("button", { name: /Unicorn/ }));
    const entry = await within(dlg).findByRole("article", { name: "Unicorn" });
    expect(within(entry).getByRole("heading", { level: 2, name: "Unicorn" })).toBeTruthy();
    const text = entry.textContent ?? "";
    const order = ["Type", "Monster", "Large celestial, lawful good", "Armor Class", "12", "Hit Points", "67 (9d10+18)", "Speed", "50 ft.",
      "STR", "CHA", "18 (+4)", "16 (+3)", "Damage Immunities", "Poison", "Condition Immunities", "Senses", "passive Perception 13",
      "Languages", "telepathy 60 ft.", "Challenge", "5 (1800 XP)", "Traits", "Charge", "Actions", "Healing Touch (3/Day)", "Legendary actions",
      "Heal Self (Costs 3 Actions)"];
    const at = order.map((s) => text.indexOf(s));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(text).not.toContain("Option");
    expect(within(entry).getByRole("columnheader", { name: "WIS" })).toBeTruthy();
    expect(within(entry).getByRole("cell", { name: "17 (+3)" })).toBeTruthy();
  });
});
