import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot } from "../api/contract";
import { renderApp, seededId } from "./utils";

/** A warlock's Pact Magic slots: shown in its band as their own tile, not as ordinary slots. */
class PactApi extends MockApi {
  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    const s = await super.loadCharacter(args);
    const classes = s.spellcasting.classes.map((c) => ({ ...c, pact: { level: 3, total: 2 } }));
    return { ...s, spellcasting: { ...s.spellcasting, classes } };
  }
}

describe("Magic band pact slots", () => {
  it("shows the class's pact slots as a short-rest tile next to the spell slots", async () => {
    const api = new PactApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const grid = await screen.findByRole("table", { name: /wizard spells/i });
    const band = within(grid.closest(".sp-main-wrap") as HTMLElement);
    const tile = band.getByRole("group", { name: "Pact slots (short rest), level 3: 2 slots" });
    expect(tile.textContent).toContain("Pact level 3 (short rest)");
    expect(band.getByRole("group", { name: /^Level 1: \d+ slots$/ })).toBeTruthy();
  });
});
