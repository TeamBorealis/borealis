import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { droppedNote, maxLevels, splitResult } from "../pages/builder/LevelSplit";
import { SRC } from "./cssRules";
import { renderApp, seededId } from "./utils";

/** B2a / #34: a character built at a higher level divides its levels between classes without levelling up one at a time. */
async function wizardAt(level: number) {
  const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
  const id = await seededId(api, "Seraphine");
  let snap = await api.getSnapshot({ characterId: id });
  while (snap.level.total < level) snap = await api.levelUp({ characterId: id, classSlotId: "c0" });
  while (snap.level.total > level) snap = await api.levelDown({ characterId: id });
  return { api, id };
}

async function openLevels(level: number) {
  const user = userEvent.setup();
  const { api, id } = await wizardAt(level);
  renderApp("/sheet", { api, openCharacterId: id });
  await user.click(await screen.findByRole("button", { name: "Level Up" }));
  const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
  const classes = async () => (await api.getSnapshot({ characterId: id })).level.classes.map((c) => [c.name, c.level]);
  return { user, api, id, dlg, classes };
}

const wizard9 = { mainName: "Wizard", mainLevel: 9, className: "Fighter", current: 0 };

describe("move levels between classes (B2a)", () => {
  it("describes the split and what it drops", () => {
    expect(maxLevels(wizard9)).toBe(8);
    expect(splitResult(wizard9, 4)).toBe("Wizard 5 / Fighter 4");
    expect(droppedNote(wizard9, 4)).toMatch(/^Choices made at Wizard levels 6 to 9 are dropped\./);
    expect(droppedNote(wizard9, 1)).toMatch(/^Choices made at Wizard level 9 are dropped\./);
    const fighter4 = { ...wizard9, mainLevel: 5, current: 4 };
    expect(droppedNote(fighter4, 4)).toBeNull();
    expect(droppedNote(fighter4, 2)).toMatch(/^Choices made at Fighter levels 3 to 4 are dropped\./);
    expect(splitResult(fighter4, 0)).toBe("Wizard 9");
  });

  it("splits a level 9 Wizard into Wizard 5 / Fighter 4 from the Multiclassing expander", async () => {
    const { user, dlg, classes } = await openLevels(9);
    await user.click(within(await within(dlg).findByLabelText("Fighter multiclass")).getByRole("button", { name: "Move levels to Fighter" }));
    const move = await screen.findByRole("dialog", { name: "Move levels to Fighter" });
    expect(within(move).getByText(/Your character stays level 9/)).toBeTruthy();
    expect(within(move).getByText("Wizard 8 / Fighter 1")).toBeTruthy();
    for (let i = 0; i < 3; i++) await user.click(within(move).getByRole("button", { name: "Increase Fighter levels" }));
    expect(within(move).getByText("Wizard 5 / Fighter 4")).toBeTruthy();
    expect(within(move).getByText(/Choices made at Wizard levels 6 to 9 are dropped/)).toBeTruthy();
    await user.click(within(move).getByRole("button", { name: "Move levels" }));

    await waitFor(async () => expect(await classes()).toEqual([["Wizard", 5], ["Fighter", 4]]));
    expect(await within(dlg).findByRole("region", { name: "Fighter 4" })).toBeTruthy();
    expect(within(dlg).getByText("Your character is level 9")).toBeTruthy();
  });

  it("moves levels back and forth on the class card, and Remove gives them back", async () => {
    const { user, api, id, dlg, classes } = await openLevels(9);
    await api.splitMulticlass({ characterId: id, levels: 4, elementId: "multi-fighter" });
    await user.click(await within(dlg).findByRole("button", { name: "Move levels to or from Fighter" }));
    const move = await screen.findByRole("dialog", { name: "Fighter levels" });
    expect(within(move).getByRole("button", { name: "Move levels" })).toHaveProperty("disabled", true);
    await user.click(within(move).getByRole("button", { name: "Decrease Fighter levels" }));
    await user.click(within(move).getByRole("button", { name: "Decrease Fighter levels" }));
    expect(within(move).getByText("Wizard 7 / Fighter 2")).toBeTruthy();
    expect(within(move).getByText(/Choices made at Fighter levels 3 to 4 are dropped/)).toBeTruthy();
    await user.click(within(move).getByRole("button", { name: "Move levels" }));
    await waitFor(async () => expect(await classes()).toEqual([["Wizard", 7], ["Fighter", 2]]));

    await user.click(await within(dlg).findByRole("button", { name: "Remove multiclass Fighter" }));
    const ask = await screen.findByRole("dialog", { name: "Remove multiclass" });
    expect(within(ask).getByText(/Its 2 levels go back to Wizard \(Wizard 9\)/)).toBeTruthy();
    await user.click(within(ask).getByRole("button", { name: "Remove multiclass" }));
    await waitFor(async () => expect(await classes()).toEqual([["Wizard", 9]]));
  });

  it("cancelling changes nothing", async () => {
    const { user, dlg, classes } = await openLevels(4);
    await user.click(within(await within(dlg).findByLabelText("Fighter multiclass")).getByRole("button", { name: "Move levels to Fighter" }));
    const move = await screen.findByRole("dialog", { name: "Move levels to Fighter" });
    expect(within(move).getByRole("button", { name: "Increase Fighter levels" })).toBeTruthy();
    await user.click(within(move).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Move levels to Fighter" })).toBeNull());
    expect(await classes()).toEqual([["Wizard", 4]]);
  });

  it("offers no move at level 1: the main class keeps a level", async () => {
    const { dlg } = await openLevels(1);
    const row = await within(dlg).findByLabelText("Fighter multiclass");
    expect(within(row).getByRole("button", { name: "Move levels to Fighter" })).toHaveProperty("disabled", true);
  });

  it("works in the phone layout: the class card links and the dialog sheet", async () => {
    document.documentElement.classList.add("phone");
    setPhoneForTests(true);
    try {
      const { user, api, id, dlg, classes } = await openLevels(6);
      await api.splitMulticlass({ characterId: id, levels: 2, elementId: "multi-fighter" });
      await user.click(await within(dlg).findByRole("button", { name: "Move levels to or from Fighter" }));
      const move = await screen.findByRole("dialog", { name: "Fighter levels" });
      await user.click(within(move).getByRole("button", { name: "Increase Fighter levels" }));
      expect(within(move).getByText("Wizard 3 / Fighter 3")).toBeTruthy();
      await user.click(within(move).getByRole("button", { name: "Move levels" }));
      await waitFor(async () => expect(await classes()).toEqual([["Wizard", 3], ["Fighter", 3]]));
      const phoneCss = readFileSync(join(SRC, "styles/phone/sheet-content.css"), "utf8");
      expect(phoneCss).toMatch(/html\.phone \.lvl-class-actions \{/);
      expect(phoneCss).toMatch(/html\.phone \.lvl-split-row \{/);
    } finally {
      setPhoneForTests(false);
      document.documentElement.classList.remove("phone");
    }
  });

  it("mock API: the main class keeps a level and is not set directly", async () => {
    const { api, id } = await wizardAt(3);
    await expect(api.splitMulticlass({ characterId: id, levels: 3 })).rejects.toMatchObject({ code: "requirement_failed" });
    await expect(api.setClassLevels({ characterId: id, classSlotId: "c0", levels: 1 })).rejects.toMatchObject({ code: "invalid_argument" });
    const split = await api.splitMulticlass({ characterId: id, levels: 2 });
    expect(split.level.total).toBe(3);
    expect(split.level.classes.map((c) => [c.name, c.level])).toEqual([["Wizard", 1], [null, 2]]);
  });
});
