import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("magic overview", () => {
  it("renders the class spell grid under the class band with V/S/M marks", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const grid = await screen.findByRole("table", { name: /wizard spells/i });
    const rows = within(grid).getAllByRole("row").slice(1);
    expect(rows.length).toBeGreaterThan(0);
    const fire = within(grid).getByRole("button", { name: "Fire Bolt" }).closest("tr")!;
    expect(within(fire).getAllByRole("cell")).toHaveLength(7);
    // Fire Bolt: verbal + somatic, no material
    expect(within(fire).getAllByRole("img", { hidden: true }).length).toBeGreaterThan(0);
    const bars = document.querySelectorAll(".sp-bar");
    expect(bars.length).toBe(1);
  });
});

describe("spellcasting tab known spells", () => {
  it("lists known spells under each class grid on the SPELLCASTING tab", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const known = await screen.findByRole("region", { name: "Known spells" });
    expect(within(known).getByRole("button", { name: /Fire Bolt/ })).toBeTruthy();
  });
});
