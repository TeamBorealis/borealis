import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { getStatus } from "../state/status";
import { renderApp, seededId } from "./utils";

async function open() {
  const user = userEvent.setup();
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  const snap = await api.getSnapshot({ characterId: id });
  const cls = snap.spellcasting.classes.find((c) => c.className === "Wizard")!;
  const spell = cls.spells.find((s) => s.level > 0 && !s.alwaysPrepared && !s.prepared)!;
  expect(spell, "seed has an unprepared wizard spell").toBeTruthy();
  renderApp("/spells", { api, openCharacterId: id });
  await user.click(await screen.findByRole("tab", { name: /wizard/i }));
  const grid = await screen.findByRole("table", { name: /wizard spells/i });
  const row = () => within(grid).getByRole("button", { name: spell.name }).closest("tr")!;
  const isPrepared = async () => (await api.getSnapshot({ characterId: id })).spellcasting.classes.find((c) => c.className === "Wizard")!.spells.find((s) => s.id === spell.id)!.prepared;
  return { user, grid, spell, row, isPrepared, cls };
}

describe("Magic class tab prepare flow", () => {
  it("drops the learn expander and shows the hint, Prepare and Refresh", async () => {
    await open();
    expect(screen.getByText("Double click a spell to prepare or unprepare it.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Refresh" })).toBeTruthy();
    expect(screen.queryByText(/learn spells/i)).toBeNull();
    expect(screen.queryByRole("region", { name: "Known spells" })).toBeNull();
  });

  it("prepares and unprepares with the button and reports it in the status bar", async () => {
    const { user, spell, isPrepared } = await open();
    await user.click(screen.getByRole("button", { name: spell.name }));
    await user.click(screen.getByRole("button", { name: "Prepare" }));
    await waitFor(async () => expect(await isPrepared()).toBe(true));
    expect(getStatus()).toBe(`Wizard: ${spell.name} is prepared.`);
    await user.click(await screen.findByRole("button", { name: "Unprepare" }));
    await waitFor(async () => expect(await isPrepared()).toBe(false));
    expect(getStatus()).toBe(`Wizard: ${spell.name} is unprepared.`);
  });

  it("toggles with double-click and with Space", async () => {
    const { user, row, spell, isPrepared } = await open();
    await user.dblClick(within(row()).getByRole("button", { name: spell.name }));
    await waitFor(async () => expect(await isPrepared()).toBe(true));
    within(row()).getByRole("button", { name: spell.name }).focus();
    await user.keyboard(" ");
    await waitFor(async () => expect(await isPrepared()).toBe(false));
    expect(getStatus()).toBe(`Wizard: ${spell.name} is unprepared.`);
  });

  it("explains cantrips and moves with ArrowDown", async () => {
    const { user, grid } = await open();
    await user.click(within(grid).getByRole("button", { name: "Fire Bolt" }));
    expect(screen.getByText("Fire Bolt (cantrip) is always ready to cast and doesn't need to be prepared.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Prepare" }) as HTMLButtonElement).disabled).toBe(true);
    const row = within(grid).getByRole("button", { name: "Fire Bolt" }).closest("tr")!;
    const next = row.nextElementSibling!.querySelector("button")!;
    within(grid).getByRole("button", { name: "Fire Bolt" }).focus();
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(next);
    expect(next.closest("tr")!.className).toContain("sel");
  });
  it("selects the row when any cell is clicked", async () => {
    const { user, grid } = await open();
    const lvl = within(grid).getByRole("button", { name: "Detect Magic" }).closest("tr")!.querySelector("td.sp-lvl")!;
    await user.click(lvl);
    expect(lvl.closest("tr")!.className).toContain("sel");
    expect((screen.getByRole("button", { name: "Prepare" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
