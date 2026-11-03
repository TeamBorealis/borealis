import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/** B-152 / #16: with average hit points off, each level chooses the average or a rolled value. */
async function openLevels(averageHp: boolean) {
  const user = userEvent.setup();
  const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
  const id = await seededId(api, "Seraphine");
  await api.setCharacterOptions({ characterId: id, options: { averageHp } });
  await api.levelUp({ characterId: id, classSlotId: "c0" });
  renderApp("/sheet", { api, openCharacterId: id });
  await user.click(await screen.findByRole("button", { name: "Level Up" }));
  const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
  const wizard = async () => (await api.getSnapshot({ characterId: id })).level.classes[0]!;
  return { user, api, id, dlg, wizard };
}

const avgButton = (dlg: HTMLElement, level: number) => within(dlg).getByRole("button", { name: `Avg hit points for Wizard level ${level}` });
const value = (dlg: HTMLElement, level: number) => within(dlg).getByLabelText(`Wizard level ${level} HP roll value`);

describe("hit points per level: average or roll (B-152)", () => {
  it("starts every later level on the average, marked (avg), with level 1 at the maximum", async () => {
    const { dlg, wizard } = await openLevels(false);
    const rolls = within(dlg).getByLabelText("Hit point rolls (d6)");
    expect(within(rolls).getByText("6 (max)")).toBeTruthy();
    expect(within(rolls).queryByRole("button", { name: /level 1$/ })).toBeNull();
    expect(value(dlg, 2).textContent).toBe("4 (avg)");
    expect(avgButton(dlg, 2).getAttribute("aria-pressed")).toBe("true");
    expect(avgButton(dlg, 3).getAttribute("aria-pressed")).toBe("true");
    expect((await wizard()).hpAverage).toEqual([false, true, true]);
  });

  it("stepping a level makes it a roll, and Avg puts it back on the average", async () => {
    const { user, dlg, wizard } = await openLevels(false);
    await user.click(within(dlg).getByRole("button", { name: "Increase Wizard level 2 HP roll" }));
    await waitFor(() => expect(value(dlg, 2).textContent).toBe("5"));
    expect(avgButton(dlg, 2).getAttribute("aria-pressed")).toBe("false");
    expect(avgButton(dlg, 3).getAttribute("aria-pressed")).toBe("true");
    let w = await wizard();
    expect(w.hpRolls).toEqual([6, 5, 4]);
    expect(w.hpAverage).toEqual([false, false, true]);
    await user.click(avgButton(dlg, 2));
    await waitFor(() => expect(value(dlg, 2).textContent).toBe("4 (avg)"));
    w = await wizard();
    expect(w.hpRolls).toEqual([6, 4, 4]);
    expect(w.hpAverage).toEqual([false, true, true]);
  });

  it("toggles from the keyboard: Avg off keeps the average value as a roll", async () => {
    const { user, dlg, wizard } = await openLevels(false);
    avgButton(dlg, 3).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(avgButton(dlg, 3).getAttribute("aria-pressed")).toBe("false"));
    expect(value(dlg, 3).textContent).toBe("4");
    const w = await wizard();
    expect(w.hpRolls).toEqual([6, 4, 4]);
    expect(w.hpAverage).toEqual([false, true, false]);
    await user.keyboard(" ");
    await waitFor(() => expect(avgButton(dlg, 3).getAttribute("aria-pressed")).toBe("true"));
  });

  it("shows no per-level controls while the average option is on", async () => {
    const { dlg } = await openLevels(true);
    expect(within(dlg).queryByLabelText("Hit point rolls (d6)")).toBeNull();
    expect(within(dlg).queryByRole("button", { name: /^Avg hit points/ })).toBeNull();
  });

  it("mock API: null clears a roll, values outside the die are refused", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    const args = { characterId: id, classSlotId: "c0", level: 2 };
    await expect(api.setHpRoll({ ...args, value: 7 })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.setHpRoll({ ...args, value: 0 })).rejects.toMatchObject({ code: "invalid_argument" });
    expect((await api.setHpRoll({ ...args, value: 2 })).level.classes[0]!.hpAverage).toEqual([false, false]);
    const cleared = await api.setHpRoll({ ...args, value: null });
    expect(cleared.level.classes[0]!.hpAverage).toEqual([false, true]);
    expect(cleared.level.classes[0]!.hpRolls).toEqual([6, 4]);
  });
});
