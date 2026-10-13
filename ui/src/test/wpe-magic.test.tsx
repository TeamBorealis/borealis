import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("magic grid", () => {
  it("lists check, NAME, LEVEL & SCHOOL, VERBAL, SOMATIC, MATERIAL and SOURCE without a duplicate known-spells list", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    await user.click(await screen.findByRole("tab", { name: /wizard/i }));
    const grid = await screen.findByRole("table", { name: /wizard spells/i });
    expect(within(grid).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["", "Name", "Level & school", "Verbal", "Somatic", "Material", "Source"]);
    const row = (await within(grid).findByRole("button", { name: "Fire Bolt" })).closest("tr")!;
    expect(within(row).getByText("Cantrip · Evocation")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Known spells" })).toBeNull();
    expect(screen.queryByText(/learn spells/i)).toBeNull();
  });
});
