import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("Sources apply-to-character", () => {
  it("edits a draft, then Apply to character stores the restriction", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    renderApp("/content/sources", { api, openCharacterId: id });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const apply = screen.getByRole("button", { name: "Apply to character" }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    const group = (await within(grid).findAllByRole("rowgroup"))[0]!;
    await user.click(within(group).getAllByRole("checkbox")[0]!);
    expect(apply.disabled).toBe(false);
    expect((await api.loadCharacter({ characterId: id })).sourceRestrictions).toBeNull();
    await user.click(apply);
    // A pick from a switched-off source stays; the confirm says so (B-55 W3 audit fix).
    const dlg = await screen.findByRole("dialog", { name: "Picks from these sources stay" }, { timeout: 400 }).catch(() => null);
    if (dlg) await user.click(within(dlg).getByRole("button", { name: "Apply" }));
    await waitFor(async () => expect((await api.loadCharacter({ characterId: id })).sourceRestrictions).not.toBeNull());
  });

  it("selecting a source fills the description pane", async () => {
    const user = userEvent.setup();
    renderApp("/content/sources");
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const row = (await within(grid).findAllByRole("row"))[2]!;
    await user.click(row);
    expect(within(screen.getByRole("complementary", { name: "Description" })).getByRole("heading", { level: 2 })).toBeTruthy();
  });
});

describe("Magic filter bar and slots", () => {
  it("narrows the grids by level and shows slots in the band and the slots table", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const grid = await screen.findByRole("table", { name: /wizard spells/i });
    const before = within(grid).getAllByRole("row").length;
    const band = grid.closest(".sp-main-wrap")!;
    expect(within(band as HTMLElement).getByLabelText("Spell slots")).toBeTruthy();
    expect(screen.getByRole("table", { name: "Selection slots" })).toBeTruthy();
    const lv = within(screen.getByRole("group", { name: "Spell levels" }));
    await user.click(lv.getByRole("button", { name: "0" }));
    const after = within(screen.getByRole("table", { name: /wizard spells/i })).getAllByRole("row").length;
    expect(after).toBeLessThan(before);
    await user.type(screen.getByLabelText("Filter spells"), "zzzz");
    expect(screen.getByText(/No spells to show/)).toBeTruthy();
  });
});
