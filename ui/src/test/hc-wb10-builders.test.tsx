import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import fixture from "../api/__fixtures__/homebrew-expr-builders.json";
import type {
  HomebrewCondition, HomebrewEquipped, HomebrewExprValue, HomebrewFormulaNode, HomebrewTagFilter,
} from "../api/contract";
import { ConditionBuilder } from "../pages/homebrew/builders/ConditionBuilder";
import { EquippedBuilder } from "../pages/homebrew/builders/EquippedBuilder";
import { FormulaBuilder } from "../pages/homebrew/builders/FormulaBuilder";
import { TagFilterBuilder } from "../pages/homebrew/builders/TagFilterBuilder";
import { DIALECT_TERMS, formulaWords, relativeProblems } from "../pages/homebrew/builders/words";
import { renderControl, Stateful } from "./hc-wb10-harness";

// Homebrew WB-10: the expression builders (spec 6.3, 6.5) on the WB-02 value shapes. Rows are chips and lists; the text form
// is never shown; kept rows are read-only with Remove; builder problems show on the row their path names.

type Cond = HomebrewExprValue<HomebrewCondition>;
const COND = fixture.condition as unknown as Cond;
const KEPT_WHOLE = fixture.conditionKept as unknown as Cond;

async function condition(initial: Cond | null, problems: { path: string; message: string }[] = []) {
  const onValue = vi.fn();
  await renderControl(<Stateful initial={initial} onValue={onValue}>{(v, set) => <ConditionBuilder label="Prerequisite" value={v} onChange={set} problems={problems} />}</Stateful>);
  return onValue;
}

const optionText = (el: HTMLElement): string => (el as HTMLSelectElement).selectedOptions[0]?.textContent ?? "";

describe("WB-10 condition builder (spec 6.3)", () => {
  it("shows every row of the WB-02 sample as chips and lists, never the text form", async () => {
    await condition(COND);
    const root = screen.getByRole("group", { name: "Prerequisite" });
    expect(optionText(within(root).getAllByRole("combobox", { name: "Prerequisite: how the parts combine" })[0]!)).toBe("All of");
    const tests = within(root).getAllByRole("combobox", { name: /: test$/ }).map(optionText);
    expect(tests).toEqual(expect.arrayContaining(["Has", "Doesn't have", "Ability score at least", "Character level at least", "Level in this class at least", "Class level at least", "Has a feature named", "Can cast spells of level", "Has an element of type", "Statistic at least"]));
    expect(document.body.textContent).not.toMatch(/\[str:13\]|&&|ID_AA_BB/);
  });

  it("a kept row is read-only with Remove only; Remove drops it and keeps the original text", async () => {
    const user = userEvent.setup();
    const onValue = await condition(COND);
    const kept = screen.getByText("Kept as is").closest("li")!;
    expect(within(kept).queryByRole("combobox")).toBeNull();
    expect(within(kept).queryByRole("textbox")).toBeNull();
    expect(within(kept).queryByRole("spinbutton")).toBeNull();
    await user.click(within(kept).getByRole("button", { name: "Remove the kept part" }));
    const next = onValue.mock.lastCall![0] as Cond;
    expect(next.original).toBe(COND.original);
    expect(JSON.stringify(next.value)).not.toContain("archetype:hunter");
  });

  it("text Borealis can't read is one kept chip with its reason; Remove leaves an empty condition", async () => {
    const user = userEvent.setup();
    const onValue = await condition(KEPT_WHOLE);
    expect(screen.getByText(/can't read this condition/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Remove the kept condition" }));
    expect(onValue).toHaveBeenLastCalledWith({ value: { shape: "model", model: { node: "group", mode: "all", items: [] } }, original: "((" });
  });

  it("adds rows and groups, changes a row's test, removes a group", async () => {
    const user = userEvent.setup();
    const onValue = await condition(null);
    await user.click(screen.getByRole("button", { name: "Add condition" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Prerequisite, part 1: test" }), "ability");
    await user.selectOptions(screen.getByRole("combobox", { name: "Prerequisite, part 1: ability" }), "dexterity");
    await user.click(screen.getByRole("button", { name: "Add group" }));
    expect((onValue.mock.lastCall![0] as Cond).value).toEqual({ shape: "model", model: { node: "group", mode: "all", items: [
      { node: "row", row: { row: "ability", ability: "dexterity", min: 13 } },
      { node: "group", mode: "any", items: [{ node: "row", row: { row: "has", id: "" } }] },
    ] } });
    await user.click(screen.getByRole("button", { name: "Remove this group" }));
    expect(((onValue.mock.lastCall![0] as Cond).value as { model: { items: unknown[] } }).model.items).toHaveLength(1);
  });

  it("class level rows pick the class from the loaded classes (lower case, as stored)", async () => {
    const user = userEvent.setup();
    const onValue = await condition(null);
    await user.click(screen.getByRole("button", { name: "Add condition" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Prerequisite, part 1: test" }), "classLevel");
    await user.selectOptions(screen.getByRole("combobox", { name: "Prerequisite, part 1: class" }), "Wizard");
    expect(JSON.stringify(onValue.mock.lastCall![0])).toContain('"class":"wizard"');
  });

  it("problems map to the row their path names (`items.2.min`), in words, with an icon", async () => {
    await condition(COND, [{ path: "items.2.min", message: "An ability score is between 1 and 30." }]);
    const row = document.querySelector<HTMLElement>('[data-path="items.2"]')!;
    expect(row.className).toContain("hb-row-bad");
    expect(within(row).getByText("An ability score is between 1 and 30.")).toBeTruthy();
    expect(document.querySelectorAll(".hb-row-bad")).toHaveLength(1);
  });

  it("draft problem paths become builder paths (`fields.requirements.items.1` -> `items.1`)", async () => {
    expect(relativeProblems([
      { severity: "error", message: "a", path: "fields.requirements.items.1", elementId: null, code: "x" },
      { severity: "info", message: "b", path: "fields.requirements.items.2", elementId: null, code: "x" },
      { severity: "error", message: "c", path: "fields.name", elementId: null, code: "x" },
    ], "fields.requirements")).toEqual([{ path: "items.1", message: "a" }]);
  });
});

describe("WB-10 tag filter builder (spec 6.3)", () => {
  it("Specific elements lists picked elements; Anything tagged shows the test rows with the dynamic chips", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={fixture.tagFilter as unknown as HomebrewExprValue<HomebrewTagFilter>} onValue={onValue}>
      {(v, set) => <TagFilterBuilder label="From" value={v} onChange={set} problems={[]} targetTypes={["Spell"]} />}</Stateful>);
    const tests = screen.getAllByRole("combobox", { name: /: test$/ }).map(optionText);
    expect(tests).toEqual(expect.arrayContaining(["Tagged", "On the class's spell list", "Of a level the class can cast"]));
    await user.selectOptions(screen.getByRole("combobox", { name: "From: from" }), "specific");
    expect(onValue).toHaveBeenLastCalledWith(expect.objectContaining({ value: { shape: "model", model: { specific: true, root: { node: "group", mode: "any", items: [] } } } }));
    expect(screen.getByRole("button", { name: "Pick: From: elements" })).toBeTruthy();
  });

  it("counts what loaded content matches a flat tag filter, and warns at zero", async () => {
    const user = userEvent.setup();
    await renderControl(<Stateful initial={null as HomebrewExprValue<HomebrewTagFilter> | null}>
      {(v, set) => <TagFilterBuilder label="From" value={v} onChange={set} problems={[]} targetTypes={["Spell"]} />}</Stateful>);
    await user.click(screen.getByRole("button", { name: "Add test" }));
    await user.type(screen.getByRole("combobox", { name: "From, part 1: tag" }), "wizard");
    expect(await screen.findByText(/\d+ match/)).toBeTruthy();
    await user.type(screen.getByRole("combobox", { name: "From, part 1: tag" }), "zzz");
    expect(await screen.findByText("Nothing loaded matches this filter.")).toBeTruthy();
  });
});

describe("WB-10 while wearing/holding builder (spec 6.3)", () => {
  it("offers each slot only the tests it can hold and keeps an item lower case", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={null as HomebrewExprValue<HomebrewEquipped> | null} onValue={onValue}>
      {(v, set) => <EquippedBuilder label="While" value={v} onChange={set} problems={[]} />}</Stateful>);
    await user.click(screen.getByRole("button", { name: "Add test" }));
    const what = screen.getByRole("combobox", { name: "While, part 1: what" });
    expect([...(what as HTMLSelectElement).options].map((o) => o.value)).toEqual(["any", "none", "light", "medium", "heavy", "item"]);
    await user.selectOptions(screen.getByRole("combobox", { name: "While, part 1: slot" }), "shield");
    expect([...(screen.getByRole("combobox", { name: "While, part 1: what" }) as HTMLSelectElement).options].map((o) => o.value)).toEqual(["any", "none", "item"]);
    await user.selectOptions(screen.getByRole("combobox", { name: "While, part 1: what" }), "item");
    await user.click(screen.getByRole("button", { name: "Pick: While, part 1: item" }));
    await user.dblClick(await within(await screen.findByRole("list", { name: "While, part 1: item options" })).findByRole("button", { name: /^Shield/ }));
    await waitFor(() => expect(JSON.stringify(onValue.mock.lastCall![0])).toMatch(/"item":"[a-z0-9_-]+"/));
  });

  it("shows the sample's kept row read-only", async () => {
    await renderControl(<EquippedBuilder label="While" value={fixture.equipped as unknown as HomebrewExprValue<HomebrewEquipped>} onChange={() => undefined} problems={[]} />);
    const kept = screen.getByText("Kept as is").closest("li")!;
    expect(within(kept).queryByRole("combobox")).toBeNull();
  });
});

describe("WB-10 formula builder (spec 6.3, hc-wb02 dialects)", () => {
  it("reads the sample in words and offers only its dialect's terms", async () => {
    await renderControl(<FormulaBuilder label="Bonus" value={fixture.formula as unknown as HomebrewExprValue<HomebrewFormulaNode>} onChange={() => undefined} dialect="formula" problems={[]} />);
    expect(screen.getByText(/^= the highest of 1, Wisdom modifier \+ Cleric level ÷ 2, rounded up/)).toBeTruthy();
    const first = screen.getAllByRole("combobox", { name: /: part$/ })[0] as HTMLSelectElement;
    expect([...first.options].map((o) => o.value)).toEqual([...DIALECT_TERMS.formula]);
  });

  it("a value field offers one term (no combining); a save DC only sums", async () => {
    const user = userEvent.setup();
    await renderControl(<Stateful initial={null as HomebrewExprValue<HomebrewFormulaNode> | null}>{(v, set) => <FormulaBuilder label="Bonus" value={v} onChange={set} dialect="value" problems={[]} />}</Stateful>);
    const pick = screen.getByRole("combobox", { name: "Bonus: part" }) as HTMLSelectElement;
    expect([...pick.options].map((o) => o.value)).toEqual(["", "number", "proficiency", "halfProficiency", "ability"]);
    await user.selectOptions(pick, "halfProficiency");
    expect(screen.getByText("= half the proficiency bonus")).toBeTruthy();
  });

  it("the save DC preset writes 8 + proficiency + an ability, and the combine list is plus only", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={null as HomebrewExprValue<HomebrewFormulaNode> | null} onValue={onValue}>{(v, set) => <FormulaBuilder label="DC" value={v} onChange={set} dialect="saveDc" problems={[]} required />}</Stateful>);
    await user.click(screen.getByRole("button", { name: "8 + proficiency + ability" }));
    expect(screen.getByText("= 8 + proficiency bonus + Wisdom modifier")).toBeTruthy();
    expect(screen.queryByRole("combobox", { name: /combined by/ })).toBeNull();
    expect(onValue.mock.lastCall![0].value.model.term).toBe("binary");
  });

  it("problems sit on the part their path names (`left.left`)", async () => {
    await renderControl(<FormulaBuilder label="Bonus" value={fixture.formula as unknown as HomebrewExprValue<HomebrewFormulaNode>} onChange={() => undefined} dialect="formula" problems={[{ path: "left.left", message: "Too big." }]} />);
    const part = document.querySelector<HTMLElement>('[data-path="left.left"]')!;
    expect(part.className).toContain("hb-row-bad");
    expect(within(part).getAllByText("Too big.")).toHaveLength(1);
  });

  it("words read classes as the user knows them", async () => {
    expect(formulaWords({ term: "ceilDiv", num: { term: "classLevel", class: "druid" }, den: { term: "number", value: 2 } }, (c) => c.toUpperCase())).toBe("DRUID level ÷ 2, rounded up");
  });
});
