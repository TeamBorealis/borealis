import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { AbilityMethod, AbilityRecord } from "../api/contract";
import { MockApi } from "../api/mock";
import { ABILITY_METHODS, abilityMethodOptions, scoresAfterMethodChange } from "../lib/abilityMethods";
import { renderApp } from "./utils";

// B-97 / GitHub #12: the ability generation method can be changed on Build > Ability Scores (U16a).

const LABEL = "Abilities generation option";
const FLYOUT_LABELS = ["Point buy", "Standard Array (15, 14, 13, 12, 10, 8)", "Roll 4d6, drop lowest", "Roll 3d6", "Manual"];
const all = (v: number): AbilityRecord<number> => ({ str: v, dex: v, con: v, int: v, wis: v, cha: v });

async function openWith(method: AbilityMethod, setup?: (api: MockApi, id: string) => Promise<unknown>) {
  const api = new MockApi({ rng: () => 0.5 });
  const snap = await api.createCharacter({ request: { name: "Method", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: method, options: {} } });
  if (setup) await setup(api, snap.id);
  const spy = vi.spyOn(api, "setAbilityMethod");
  renderApp("/builder/abilities", { api, openCharacterId: snap.id });
  const select = (await screen.findByLabelText(LABEL)) as HTMLSelectElement;
  return { api, id: snap.id, spy, select };
}

const base = async (api: MockApi, id: string) => (await api.getSnapshot({ characterId: id })).abilities.base;

describe("method choices (shared with the New Character flyout)", () => {
  it("lists the flyout's choices in the flyout's order", () => {
    expect(abilityMethodOptions.map((o) => o.label)).toEqual(FLYOUT_LABELS);
    expect(abilityMethodOptions.map((o) => o.value)).toEqual(ABILITY_METHODS);
  });

  it("predicts the engine's reset: point buy to 8s, the standard array in order, other methods keep the scores", () => {
    const cur = { str: 17, dex: 12, con: 14, int: 9, wis: 10, cha: 8 };
    expect(scoresAfterMethodChange(cur, "point_buy")).toEqual(all(8));
    expect(scoresAfterMethodChange(cur, "standard_array")).toEqual({ str: 15, dex: 14, con: 13, int: 12, wis: 10, cha: 8 });
    for (const m of ["manual", "roll_3d6", "roll_4d6"] as const) expect(scoresAfterMethodChange(cur, m)).toEqual(cur);
  });
});

describe("Abilities tab method control", () => {
  it.each(ABILITY_METHODS)("renders for %s with the current method selected and every flyout choice", async (method) => {
    const { select } = await openWith(method);
    expect(select.value).toBe(method);
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toEqual(FLYOUT_LABELS);
  });

  it("switches without asking when no score would change (manual at 8s to point buy)", async () => {
    const { api, id, spy, select } = await openWith("point_buy");
    const user = userEvent.setup();
    await user.selectOptions(select, "manual");
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ characterId: id, method: "manual" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.selectOptions(await screen.findByLabelText(LABEL), "point_buy");
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await base(api, id)).toEqual(all(8));
  });

  it("switches from manual to a roll method without asking and keeps the scores", async () => {
    const { api, id, spy, select } = await openWith("manual", (a, cid) => a.setBaseScore({ characterId: cid, ability: "str", value: 17 }));
    const user = userEvent.setup();
    await user.selectOptions(select, "roll_4d6");
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ characterId: id, method: "roll_4d6" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect((await base(api, id)).str).toBe(17);
  });

  it("asks before point buy would reset typed scores, and Cancel keeps everything", async () => {
    const { api, id, spy, select } = await openWith("manual", (a, cid) => a.setBaseScore({ characterId: cid, ability: "str", value: 17 }));
    const user = userEvent.setup();
    await user.selectOptions(select, "point_buy");
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toMatch(/go back to 8/);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(spy).not.toHaveBeenCalled();
    expect(((await screen.findByLabelText(LABEL)) as HTMLSelectElement).value).toBe("manual");
    expect((await base(api, id)).str).toBe(17);
  });

  it("asks before the standard array would replace the scores, and Change applies it", async () => {
    const { api, id, spy, select } = await openWith("point_buy");
    const user = userEvent.setup();
    await user.selectOptions(select, "standard_array");
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toMatch(/15, 14, 13, 12, 10 and 8/);
    await user.click(within(dialog).getByRole("button", { name: "Change" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ characterId: id, method: "standard_array" }));
    expect(await base(api, id)).toEqual({ str: 15, dex: 14, con: 13, int: 12, wis: 10, cha: 8 });
    await waitFor(() => expect(((screen.getByLabelText(LABEL)) as HTMLSelectElement).value).toBe("standard_array"));
  });

  it("keeps the point buy budget, roll and reset controls next to the method", async () => {
    await openWith("point_buy");
    expect(screen.getByRole("status", { name: "Point buy budget" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reset ability scores" })).toBeTruthy();
  });

  it("keeps the roll controls for a roll method", async () => {
    await openWith("roll_3d6");
    expect(screen.getByRole("button", { name: "Roll all abilities" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reset ability scores" })).toBeTruthy();
  });
});

describe("manual entry", () => {
  it("types a base score into the field (Enter commits)", async () => {
    const { api, id } = await openWith("manual");
    const user = userEvent.setup();
    const field = screen.getByRole("textbox", { name: "Strength base score" });
    await user.clear(field);
    await user.type(field, "17{Enter}");
    await waitFor(async () => expect((await base(api, id)).str).toBe(17));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Strength base score" })).toHaveProperty("value", "17"));
  });

  it("commits on leaving the field and clamps to the allowed range", async () => {
    const { api, id } = await openWith("manual");
    const user = userEvent.setup();
    const field = screen.getByRole("textbox", { name: "Wisdom base score" });
    await user.clear(field);
    await user.type(field, "45");
    fireEvent.blur(field);
    await waitFor(async () => expect((await base(api, id)).wis).toBe(30));
  });

  it("puts the score back on Escape or an empty entry", async () => {
    const { api, id } = await openWith("manual");
    const user = userEvent.setup();
    const field = screen.getByRole("textbox", { name: "Dexterity base score" });
    await user.clear(field);
    await user.type(field, "12{Escape}");
    expect((field as HTMLInputElement).value).toBe("10");
    await user.clear(field);
    fireEvent.blur(field);
    expect((field as HTMLInputElement).value).toBe("10");
    expect((await base(api, id)).dex).toBe(10);
  });

  it("keeps the chevrons working beside the field", async () => {
    const { api, id } = await openWith("manual");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Increase Charisma" }));
    await waitFor(async () => expect((await base(api, id)).cha).toBe(11));
  });

  it("has no typing field for methods that set the scores themselves", async () => {
    await openWith("point_buy");
    expect(screen.queryByRole("textbox", { name: "Strength base score" })).toBeNull();
  });
});

// Layout rules checked with Playwright at 1355x847, 390x844, 844x390 and 1280x720 (specs/visual-refs/b97/).
describe("layout (no horizontal scroll)", () => {
  const css = readFileSync(resolve(__dirname, "..", "pages/builder/builder.css"), "utf8");
  const phone = css.slice(css.indexOf("@media (max-width: 899px) {"));

  it("keeps the method band in the page flow so it cannot cover the tiles", () => {
    expect(css).toMatch(/\.abil-top \{ display: flex;[^}]*flex-wrap: wrap/);
    expect(css).not.toMatch(/\.abil-top \{[^}]*position: absolute/);
  });

  it("narrows the six-column pitch before the Charisma tile leaves the pane (B-96b)", () => {
    expect(css).toMatch(/\.abil-row \{ display: grid; grid-template-columns: repeat\(6, minmax\(126px, 146px\)\)/);
  });

  it("lets the method field shrink to the phone width", () => {
    expect(phone).toMatch(/\.abil-method \{[^}]*max-width: 420px[^}]*flex-direction: column/);
    expect(phone).toMatch(/\.abil-method-combo \{ width: 100%; \}/);
  });
});
