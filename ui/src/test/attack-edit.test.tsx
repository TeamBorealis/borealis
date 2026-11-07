import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/** Brannoc (STR 16, DEX 12) holds a Longsword: its row is automatic. */
async function openAttacks() {
  const api = new MockApi();
  const id = await seededId(api, "Brannoc");
  const user = userEvent.setup();
  renderApp("/manage", { api, openCharacterId: id });
  await user.click(await screen.findByRole("tab", { name: "Attacks & Spellcasting" }));
  return { api, id, user };
}

describe("editing a weapon attack (GitHub #27)", () => {
  it("recalculates the bonus and damage when another ability is picked, and saves it", async () => {
    // Arrange
    const { api, id, user } = await openAttacks();
    await user.click(await screen.findByRole("button", { name: "Edit Longsword" }));
    const dialog = await screen.findByRole("dialog");
    const damage = within(dialog).getByLabelText("Damage") as HTMLInputElement;
    expect(damage.value).toBe("1d8+3 slashing");
    expect(within(dialog).queryByLabelText("Calculate automatically")).toBeNull();

    // Act
    await user.selectOptions(within(dialog).getByLabelText("Ability (automatic calculation)"), "dex");

    // Assert: the dialog follows the new ability at once
    expect(damage.value).toBe("1d8+1 slashing");
    expect(within(dialog).getByText("Calculated with Dexterity: 1d8+1 slashing")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(async () => {
      const row = (await api.getSnapshot({ characterId: id })).attacks.find((a) => a.name === "Longsword")!;
      expect(row.ability).toBe("dex");
      expect(row.damage).toBe("1d8+1 slashing");
    });
  });

  it("keeps a typed damage when the ability changes", async () => {
    const { user } = await openAttacks();
    await user.click(await screen.findByRole("button", { name: "Edit Longsword" }));
    const dialog = await screen.findByRole("dialog");
    const damage = within(dialog).getByLabelText("Damage") as HTMLInputElement;
    await user.clear(damage);
    await user.type(damage, "2d6 radiant");
    await user.selectOptions(within(dialog).getByLabelText("Ability (automatic calculation)"), "dex");
    expect(damage.value).toBe("2d6 radiant");
  });

  it("can take a weapon row off the sheet from the list", async () => {
    const { api, id, user } = await openAttacks();
    await user.click(await screen.findByRole("button", { name: "Hide Longsword from the sheet" }));
    await waitFor(async () => {
      const row = (await api.getSnapshot({ characterId: id })).attacks.find((a) => a.name === "Longsword")!;
      expect(row.showInCombat).toBe(false);
    });
  });
});
