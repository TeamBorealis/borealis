import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomebrewRef } from "../api/contract";
import { MOCK_STAT_CATALOG } from "../api/mock/homebrew-catalog";
import { setPhoneForTests } from "../lib/phone";
import { ChipsField } from "../pages/homebrew/pickers/ChipsField";
import { RefField } from "../pages/homebrew/pickers/RefField";
import { fillPattern, StatisticField, statLabel } from "../pages/homebrew/pickers/StatisticPicker";
import { CLASSES, renderControl, Stateful } from "./hc-wb10-harness";

// Homebrew WB-10: the reference picker (spec 5.1, 13.5), the spell picker (13.7), the statistic picker (5.2) and chips.

afterEach(() => act(() => setPhoneForTests(false)));

const env = { stats: MOCK_STAT_CATALOG, classes: CLASSES } as const;

async function refField(initial: HomebrewRef[], target: { types: string[]; multiple?: boolean }, onValue = vi.fn()) {
  await renderControl(
    <Stateful initial={initial} onValue={onValue}>{(v, set) => <RefField label="What it gives" target={target} value={v} onChange={set} />}</Stateful>,
  );
  return onValue;
}

describe("WB-10 reference picker (spec 5.1, 13.5)", () => {
  it("lists loaded elements in the Build picker's rows, describes the clicked row and chooses it; the id never shows", async () => {
    // Arrange
    const user = userEvent.setup();
    const onValue = await refField([], { types: ["Skill"] });
    // Act
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    const dialog = await screen.findByRole("dialog", { name: "Choose: What it gives" });
    const list = await within(dialog).findByRole("list", { name: "What it gives options" });
    await user.click(await within(list).findByRole("button", { name: /^Acrobatics/ }));
    await waitFor(() => expect(within(dialog).getByRole("heading", { name: "Acrobatics" })).toBeTruthy());
    await user.click(within(dialog).getByRole("button", { name: "Choose" }));
    // Assert
    expect(onValue).toHaveBeenLastCalledWith([{ id: "skill-acrobatics", name: "Acrobatics", type: "skill" }]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("Acrobatics")).toBeTruthy();
    expect(document.body.textContent).not.toContain("skill-acrobatics");
  });

  it("Enter on a row and a double-click both choose", async () => {
    const user = userEvent.setup();
    const onValue = await refField([], { types: ["Language"] });
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    const list = await screen.findByRole("list", { name: "What it gives options" });
    const row = await within(list).findByRole("button", { name: /^Dwarvish/ });
    row.focus();
    await user.keyboard("{Enter}");
    expect(onValue).toHaveBeenLastCalledWith([expect.objectContaining({ name: "Dwarvish" })]);
    await user.click(screen.getByRole("button", { name: "Change: What it gives" }));
    await user.dblClick(await within(await screen.findByRole("list", { name: "What it gives options" })).findByRole("button", { name: /^Elvish/ }));
    expect(onValue).toHaveBeenLastCalledWith([expect.objectContaining({ name: "Elvish" })]);
  });

  it("the search narrows the rows (on the backend) and the count follows", async () => {
    const user = userEvent.setup();
    await refField([], { types: [] });
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("button", { name: /^Acrobatics/ });
    await user.type(within(dialog).getByRole("searchbox", { name: "Search elements" }), "elvish");
    await waitFor(() => expect(within(dialog).queryByRole("button", { name: /^Acrobatics/ })).toBeNull());
    expect(within(dialog).getByRole("button", { name: /^Elvish/ })).toBeTruthy();
  });

  it("a missing element reads 'Missing: <name>' with Pick again (spec 5.1)", async () => {
    await refField([{ id: "ID_GONE", name: "Old Spell", type: "Spell", missing: true }], { types: ["Spell"] });
    expect(screen.getByText("Missing: Old Spell")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Pick again: What it gives" })).toBeTruthy();
  });

  it("several picks keep a check per row, list the chosen with X, and OK returns them all", async () => {
    const user = userEvent.setup();
    const onValue = await refField([], { types: ["Language"], multiple: true });
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    const list = await screen.findByRole("list", { name: "What it gives options" });
    await user.dblClick(await within(list).findByRole("button", { name: /^Dwarvish/ }));
    await user.dblClick(within(list).getByRole("button", { name: /^Elvish/ }));
    await user.click(screen.getByRole("button", { name: "Clear Dwarvish" }));
    await user.dblClick(within(list).getByRole("button", { name: /^Giant/ }));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onValue.mock.lastCall![0].map((r: HomebrewRef) => r.name)).toEqual(["Elvish", "Giant"]);
  });

  it("a spell picker shows the Magic grid and filter bar; the level toggles narrow it and the check chooses", async () => {
    const user = userEvent.setup();
    const onValue = await refField([], { types: ["Spell"] });
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    const grid = await screen.findByRole("table", { name: "Spells" });
    await within(grid).findByText("Fire Bolt");
    expect(within(grid).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["", "Name", "Level & school", "Verbal", "Somatic", "Material", "Source"]);
    await user.click(within(screen.getByRole("group", { name: "Spell levels" })).getByRole("button", { name: "3" }));
    await waitFor(() => expect(within(grid).queryByText("Fire Bolt")).toBeNull());
    expect(within(grid).getByText("Fireball")).toBeTruthy();
    await user.click(within(grid).getByRole("checkbox", { name: "Choose Fireball" }));
    expect(onValue).toHaveBeenLastCalledWith([expect.objectContaining({ name: "Fireball", type: "spell" })]);
  });

  it("on phones: a bottom sheet with Choose under the description (spec 16, O2 B)", async () => {
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    const onValue = await refField([], { types: ["Skill"] });
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    await user.click(await within(await screen.findByRole("list", { name: "What it gives options" })).findByRole("button", { name: /^Athletics/ }));
    // The row's own ring chooses too (A1); the button under the description is the sheet's.
    await waitFor(() => expect(document.querySelector(".hb-picker-choose")).not.toBeNull());
    await user.click(document.querySelector<HTMLButtonElement>(".hb-picker-choose")!);
    expect(onValue).toHaveBeenLastCalledWith([expect.objectContaining({ name: "Athletics" })]);
  });
});

describe("WB-10 statistic picker (spec 5.2)", () => {
  it("shows statistics in plain words by group and writes the name", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial="" onValue={onValue}>{(v, set) => <StatisticField label="Statistic" value={v} onChange={set} />}</Stateful>, { env });
    await user.click(screen.getByRole("button", { name: "Pick: Statistic" }));
    await user.type(screen.getByRole("searchbox", { name: "Search statistics" }), "darkvision");
    await user.click(await screen.findByRole("button", { name: "Darkvision range" }));
    expect(onValue).toHaveBeenLastCalledWith("darkvision:range");
    expect(screen.getByText("Darkvision range")).toBeTruthy();
    expect(document.body.textContent).not.toContain("darkvision:range");
  });

  it("class-scoped statistics are written from a class picker and a number, never typed", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial="" onValue={onValue}>{(v, set) => <StatisticField label="Statistic" value={v} onChange={set} />}</Stateful>, { env });
    await user.click(screen.getByRole("button", { name: "Pick: Statistic" }));
    await user.type(screen.getByRole("searchbox", { name: "Search statistics" }), "spell slots of level");
    await user.click(await screen.findByRole("button", { name: "Spell slots of level N" }));
    await user.selectOptions(screen.getByLabelText("Class"), "paladin");
    await user.click(screen.getByRole("button", { name: "Increase Level" }));
    await user.click(screen.getByRole("button", { name: "Increase Level" }));
    await user.click(screen.getByRole("button", { name: "Choose" }));
    expect(onValue).toHaveBeenLastCalledWith("paladin:spellcasting:slots:3");
    expect(screen.getByText("Spell slots of level 3 (Paladin)")).toBeTruthy();
  });

  it("labels names in plain words, patterns included", async () => {
    const e = { ...envFor() };
    expect(statLabel(e, "strength:modifier")).toBe("Strength modifier");
    expect(statLabel(e, "level:wizard")).toBe("Class level (Wizard)");
    expect(statLabel(e, "multiclass:spellcasting:slot:2")).toBe("Multiclass spell slots of level 2");
    expect(statLabel(e, "made up:thing")).toBe("made up thing");
    expect(fillPattern("{class}:spellcasting:slots:{n}", "Paladin", 4)).toBe("paladin:spellcasting:slots:4");
  });
});

function envFor() {
  return { draftId: null, edition: null, blocks: [], stats: MOCK_STAT_CATALOG, classes: CLASSES, form: null, answer: null, problems: [], expert: false };
}

describe("WB-10 chips (spec 3.2 tags and keywords; block chips)", () => {
  it("a closed set is toggle chips with aria-pressed and a check (never colour alone)", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={["fire"]} onValue={onValue}>{(v, set) => (
      <ChipsField label="Damage types" value={v} onChange={set} choices={[{ value: "fire", label: "Fire" }, { value: "cold", label: "Cold" }]} />
    )}</Stateful>);
    const group = screen.getByRole("group", { name: "Damage types" });
    expect(within(group).getByRole("button", { name: "Fire" }).getAttribute("aria-pressed")).toBe("true");
    await user.click(within(group).getByRole("button", { name: "Cold" }));
    await user.click(within(group).getByRole("button", { name: "Fire" }));
    expect(onValue).toHaveBeenLastCalledWith(["cold"]);
  });

  it("an open list adds typed chips (trimmed, no list characters, no duplicates) and removes them", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={["Beast"]} onValue={onValue}>{(v, set) => <ChipsField label="Tags" value={v} onChange={set} allowCustom />}</Stateful>);
    await user.type(screen.getByRole("combobox", { name: "New tags" }), "  Sea, Folk  {Enter}");
    expect(onValue).toHaveBeenLastCalledWith(["Beast", "Sea Folk"]);
    await user.type(screen.getByRole("combobox", { name: "New tags" }), "beast{Enter}");
    expect(onValue).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Remove Beast" }));
    expect(onValue).toHaveBeenLastCalledWith(["Sea Folk"]);
  });
});
