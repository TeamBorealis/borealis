import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

async function setup(route: string) {
  const api = new MockApi({ aiChunkDelayMs: 2 });
  const snap = await api.createCharacter({ request: { name: "Fx2", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  renderApp(route, { api, openCharacterId: snap.id });
}

describe("build pickers", () => {
  it("sorts picker rows by name", async () => {
    await setup("/builder/class");
    const list = await screen.findByRole("list", { name: "Class options" });
    await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(1));
    const names = within(list).getAllByRole("listitem").map((li) => li.querySelector(".pick-name")!.textContent ?? "");
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b)));
  });

  it("tag button navigates to START > SOURCES and no inline filter exists", async () => {
    const user = userEvent.setup();
    await setup("/builder/class");
    await screen.findByRole("list", { name: "Class options" });
    expect(document.querySelector(".pick-filter")).toBeNull();
    await user.click((await screen.findAllByRole("button", { name: "Manage Sources" }))[0]!);
    await waitFor(() => expect(screen.queryByRole("list", { name: "Class options" })).toBeNull());
    expect((await screen.findAllByRole("row")).length).toBeGreaterThan(0);
  });

  it("source link opens the compendium flyout", async () => {
    const user = userEvent.setup();
    await setup("/builder/class");
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.click(await within(list).findByRole("button", { name: /Fighter/ }));
    const link = await waitFor(() => {
      const el = document.querySelector<HTMLButtonElement>("button.desc-source");
      if (!el) throw new Error("no link");
      return el;
    });
    await user.click(link);
    expect(await screen.findByLabelText("Filter by source")).toBeTruthy();
  });
});
