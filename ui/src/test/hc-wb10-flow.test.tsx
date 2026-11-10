import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomebrewForm, HomebrewSheetValue } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { SheetField, templateParts, templateText } from "../pages/homebrew/fields/FormParts";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";
import { renderApp } from "./utils";

// Homebrew WB-10 in the creator on the mock: the controls inside the WB-09 shell (General, Rules, Description), their undo
// steps, the creator keys for blocks, problems from the draft mapped onto rows, the expert fields, and phones.

afterEach(() => act(() => setPhoneForTests(false)));

const editor = () => screen.getByRole("region", { name: "Element editor" });
const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;

async function newSpell(user: ReturnType<typeof userEvent.setup>) {
  const api = mockApi();
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const update = vi.spyOn(api, "homebrewUpdateDraft");
  renderApp(`/homebrew/${pack.id}`, { api });
  // Desktop: the empty editor's New element; phones: the list's New.
  await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.type(within(flyout).getByLabelText("Name"), "Ember Bolt");
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));
  const lastForm = (): HomebrewForm => update.mock.lastCall![0].form;
  return { api, update, lastForm };
}

describe("WB-10 controls in the creator (mock)", () => {
  it("prerequisite: a row the draft's check refuses is marked with its sentence; picking the element clears it", async () => {
    const user = userEvent.setup();
    const { lastForm } = await newSpell(user);
    await user.click(within(editor()).getByRole("button", { name: "Add condition" }));
    // The mock's live check answers with `fields.requirements.items.0`; the row shows it, PROBLEMS counts it.
    const row = await waitFor(() => { const r = document.querySelector<HTMLElement>('[data-field="requirements"] [data-path="items.0"]'); expect(r?.className).toContain("hb-row-bad"); return r!; });
    expect(within(row).getByText("Pick the element this condition tests.")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Problems \(1\)/ })).toBeTruthy();
    await user.click(within(row).getByRole("button", { name: "Pick: Prerequisite, part 1: element" }));
    await user.dblClick(await within(await screen.findByRole("list", { name: "Prerequisite, part 1: element options" })).findByRole("button", { name: /^Wizard/ }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());
    expect(lastForm().fields.requirements).toEqual({ value: { shape: "model", model: { node: "group", mode: "all", items: [{ node: "row", row: { row: "has", id: "wizard" } }] } } });
  });

  it("rules: Insert opens the Add menu on RULES, the block is one undo step, Ctrl+Z takes it back", async () => {
    const user = userEvent.setup();
    const { lastForm } = await newSpell(user);
    fireEvent.keyDown(window, { key: "Insert" });
    const menu = await screen.findByRole("dialog", { name: "Add a rule" });
    expect(within(editor()).getByRole("tab", { name: "RULES" }).getAttribute("aria-selected")).toBe("true");
    await user.click(within(menu).getByRole("button", { name: "Advantage on a roll" }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Advantage on a roll" });
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    const list = await screen.findByRole("list", { name: "Rules" });
    expect(list.textContent).toContain("Advantage on a roll");
    await waitFor(() => expect(lastForm().blocks).toEqual([expect.objectContaining({ kind: "roll_advantage", fields: { roll: "ability_check" } })]));
    await user.click(within(editor()).getByRole("button", { name: "Undo" }));
    expect(screen.queryByRole("list", { name: "Rules" })).toBeNull();
    await waitFor(() => expect(lastForm().blocks).toEqual([]));
  });

  it("description: the rich-text editor is the DESCRIPTION control and the preview follows it", async () => {
    const user = userEvent.setup();
    const { lastForm } = await newSpell(user);
    await user.click(within(editor()).getByRole("tab", { name: "DESCRIPTION" }));
    expect(within(editor()).getByRole("toolbar", { name: "Description formatting" })).toBeTruthy();
    expect(within(editor()).getByRole("button", { name: /higher-level/i })).toBeTruthy();
    await user.click(within(editor()).getByRole("button", { name: /higher-level/i }));
    await waitFor(() => expect(JSON.stringify(lastForm().fields.description)).toContain("Using a Higher-Level Spell Slot."));
    const pane = screen.getAllByRole("complementary", { name: "Description" }).at(-1)!;
    await waitFor(() => expect(pane.textContent).toContain("Using a Higher-Level Spell Slot."));
  });

  it("expert fields stay hidden until Expert is on: the read-only Source and the counterpart", async () => {
    const user = userEvent.setup();
    await newSpell(user);
    expect(document.querySelector('[data-field="source"]')).toBeNull();
    await user.click(within(editor()).getByRole("switch", { name: "Expert fields" }));
    const source = document.querySelector<HTMLElement>('[data-field="source"]')!;
    expect(within(source).queryByRole("textbox")).toBeNull();
    await user.click(within(editor()).getByRole("tab", { name: "DESCRIPTION" }));
    expect(document.querySelector('[data-field="mapsTo"]')).not.toBeNull();
  });

  it("phones: block dialog selects open the list sheet (U22)", async () => {
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await newSpell(user);
    await user.click(within(editor()).getByRole("tab", { name: "RULES" }));
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Advantage on a roll" }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Advantage on a roll" });
    // No native select on phones (editable combos stay text boxes with suggestions).
    expect(dialog.querySelector("select")).toBeNull();
    const roll = dialog.querySelector<HTMLButtonElement>('[data-field="roll"] .ph-select')!;
    await user.click(roll);
    const sheet = await screen.findByRole("listbox");
    await user.click(within(sheet).getByRole("option", { name: "Saving throws" }));
    expect(roll.textContent).toBe("Saving throws");
  });
});

describe("WB-10 sheet lines and value chips (spec 7.4)", () => {
  it("values go in as chips from the statistic picker; typed braces are refused", async () => {
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(<Stateful initial={null as HomebrewSheetValue | null} onValue={(v) => onValue(v)}>{(v, set) => <SheetField id="s" value={v} onChange={(next) => set(next)} />}</Stateful>);
    await user.click(screen.getByRole("button", { name: "Add a sheet line" }));
    await user.type(screen.getByRole("textbox", { name: "Sheet line 1" }), "Rage {x} ");
    await user.click(screen.getByRole("button", { name: "Insert value" }));
    await user.type(screen.getByRole("searchbox", { name: "Search statistics" }), "rages");
    await user.click(await screen.findByRole("button", { name: "Rages" }));
    expect(onValue.mock.lastCall![0].lines[0].text).toBe("Rage x {{barbarian rage:count}}");
    expect(screen.getByText("Rages")).toBeTruthy();
    expect(document.body.textContent).not.toContain("{{");
  });

  it("splits and joins template text", async () => {
    expect(templateParts("A {{b}} c")).toEqual([{ text: "A " }, { stat: "b" }, { text: " c" }]);
    expect(templateText(templateParts("{{x}}{{y}}"))).toBe("{{x}}{{y}}");
  });
});
