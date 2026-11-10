import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewBlock, HomebrewForm, HomebrewProblem } from "../api/contract";
import { MOCK_BLOCK_CATALOG } from "../api/mock/homebrew-catalog";
import { addMenuGroups, offeredBlocks } from "../pages/homebrew/blocks/AddBlockDialog";
import { BlockDialog, PreservedList } from "../pages/homebrew/blocks/BlockDialog";
import { BlockList, blockProblems } from "../pages/homebrew/blocks/BlockList";
import { shownSentence } from "../pages/homebrew/blocks/sentence";
import { cleanFields, missingRequired, newBlock, numberRows } from "../pages/homebrew/blocks/values";
import { renderControl, Stateful } from "./hc-wb10-harness";

// Homebrew WB-10: the rule block list (spec 6, 13.6: the Attacks list editor) and the generic block dialog rendered from
// the real catalog (`HomebrewBlockSpec`, WB-05).

const spec = (kind: string) => MOCK_BLOCK_CATALOG.find((b) => b.kind === kind)!;
const block = (kind: string, fields: Record<string, unknown>, extra: Partial<HomebrewBlock> = {}): HomebrewBlock => ({ key: `k-${kind}`, kind, sentence: `${spec(kind).label} sentence`, fields, ...extra });

async function list(initial: HomebrewBlock[], problems: HomebrewProblem[] = [], answer: HomebrewForm | null = null) {
  const onValue = vi.fn();
  await renderControl(<Stateful initial={initial} onValue={onValue}>{(v, set) => <BlockList label="Rules" blocks={v} onChange={set} problems={problems} answer={answer} />}</Stateful>);
  return onValue;
}

const ROLL = block("roll_advantage", { roll: "saving_throw" });
const SENSE = block("sense", { sense: "darkvision", range: 60 });

describe("WB-10 Add menu (spec 6.2)", () => {
  it("lists Common first, then the groups; effect-only and core-rule kinds stay out", async () => {
    const groups = addMenuGroups(offeredBlocks(MOCK_BLOCK_CATALOG, undefined, false));
    expect(groups[0]!.label).toBe("Common");
    expect(groups[0]!.blocks.map((b) => b.kind)).toEqual(MOCK_BLOCK_CATALOG.filter((b) => b.common).map((b) => b.kind));
    const kinds = groups.flatMap((g) => g.blocks.map((b) => b.kind));
    expect(kinds).not.toContain("speed_multiply");
    expect(kinds).not.toContain("exhaustion");
    expect(new Set(kinds).size).toBe(kinds.length);
    expect(offeredBlocks(MOCK_BLOCK_CATALOG, undefined, true).map((b) => b.kind)).toContain("exhaustion");
    expect(offeredBlocks(MOCK_BLOCK_CATALOG, ["grant", "stat", "roll_advantage", "speed_multiply"], false).map((b) => b.kind)).toEqual(["grant", "stat", "roll_advantage", "speed_multiply"]);
  });
});

describe("WB-10 block list (spec 6, 13.6)", () => {
  it("adds a rule: Add rule -> pick a kind -> fill the dialog -> OK appends it with a sentence", async () => {
    const user = userEvent.setup();
    const onValue = await list([]);
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Special sense" }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Special sense" });
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    // Required fields first: the dialog stays and says so.
    expect(within(dialog).getByRole("alert").textContent).toMatch(/Fill in/);
    // An open set (custom senses allowed): typed or picked by its words, stored as the engine's value.
    await user.type(within(dialog).getByRole("combobox", { name: /^Sense/ }), "Darkvision");
    await user.click(within(dialog).getByRole("button", { name: "Set: Range (feet)" }));
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    const blocks = onValue.mock.lastCall![0] as HomebrewBlock[];
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ kind: "sense", fields: { sense: "darkvision", range: 0 } });
    expect(screen.getByRole("list", { name: "Rules" }).textContent).toContain("Special sense");
  });

  it("moves with the toolbar and Alt+Up/Down, edits with Enter, removes with Delete (spec 12.2)", async () => {
    const user = userEvent.setup();
    const onValue = await list([ROLL, SENSE]);
    const rows = () => screen.getAllByRole("listitem").filter((li) => li.classList.contains("hb-block"));
    await user.click(rows()[1]!);
    await user.click(screen.getByRole("button", { name: "Move selected rule up" }));
    expect((onValue.mock.lastCall![0] as HomebrewBlock[]).map((b) => b.kind)).toEqual(["sense", "roll_advantage"]);
    act(() => rows()[0]!.focus());
    fireEvent.keyDown(rows()[0]!, { key: "ArrowDown", altKey: true });
    expect((onValue.mock.lastCall![0] as HomebrewBlock[]).map((b) => b.kind)).toEqual(["roll_advantage", "sense"]);
    // Focus follows the moved block.
    await waitFor(() => expect(document.activeElement).toBe(rows()[1]));
    act(() => rows()[0]!.focus());
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: "Advantage on a roll" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    act(() => rows()[0]!.focus());
    await user.keyboard("{Delete}");
    expect((onValue.mock.lastCall![0] as HomebrewBlock[]).map((b) => b.kind)).toEqual(["sense"]);
  });

  it("names each block by its sentence and shows the backend's sentence while the block is unchanged", async () => {
    const answer: HomebrewForm = { kind: "spell", fields: {}, blocks: [{ ...SENSE, sentence: "Darkvision 60 feet" }], children: [], preserved: [] };
    await list([SENSE], [], answer);
    expect(screen.getByRole("listitem", { name: "Darkvision 60 feet" })).toBeTruthy();
    expect(shownSentence({ ...SENSE, fields: { sense: "darkvision", range: 90 } }, answer)).toBe(SENSE.sentence);
  });

  it("marks a block with problems (icon and words) and passes its field problems to the dialog", async () => {
    const user = userEvent.setup();
    const problems: HomebrewProblem[] = [{ severity: "error", message: "Sense: fill in range.", path: "blocks.1.fields.range", elementId: null, code: "x" }];
    await list([ROLL, SENSE], problems);
    const bad = document.querySelector<HTMLElement>('[data-block="1"]')!;
    expect(bad.className).toContain("hb-block-bad");
    expect(within(bad).getByText("Sense: fill in range.")).toBeTruthy();
    expect(blockProblems(problems, 1)).toEqual([{ path: "range", message: "Sense: fill in range." }]);
    await user.click(within(bad).getByRole("button", { name: /^Edit:/ }));
    const field = document.querySelector<HTMLElement>('.hb-block-dialog [data-field="range"]')!;
    expect(field.className).toContain("hb-error");
  });

  it("Cancel on a changed rule asks first; on an unchanged one it closes at once (spec 16, O4 B)", async () => {
    const user = userEvent.setup();
    await list([SENSE]);
    await user.click(screen.getByRole("button", { name: /^Edit:/ }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Edit:/ }));
    await user.click(screen.getByRole("button", { name: "Increase Range (feet)" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("dialog", { name: "Discard changes?" })).toBeTruthy();
  });

  it("the Insert key's event opens the Add menu on the top-level list", async () => {
    await list([]);
    act(() => { window.dispatchEvent(new Event("hb-add-block")); });
    expect(await screen.findByRole("dialog", { name: "Add a rule" })).toBeTruthy();
  });
});

describe("WB-10 generic block dialog (spec 6.1-6.4)", () => {
  it.each(MOCK_BLOCK_CATALOG.map((b) => [b.kind]))("%s opens with a labelled control for every field it offers", async (kind) => {
    const s = spec(kind);
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={() => undefined} onCancel={() => undefined} />);
    const dialog = screen.getByRole("dialog");
    for (const f of s.fields.filter((x) => x.control !== "auto" && !x.expert)) {
      expect(dialog.querySelector(`[data-field="${f.key}"]`), `${kind}.${f.key}`).not.toBeNull();
    }
    expect(!!within(dialog).queryByText("From level")).toBe(s.levelGated);
    expect(!!within(dialog).queryByText("Only if")).toBe(s.onlyIf);
    expect(dialog.textContent).not.toMatch(/\bID_|\$\(|\{\{/);
  });

  it("the Expert toggle shows the rarely used fields (select: can be replaced later)", async () => {
    const user = userEvent.setup();
    const s = spec("select");
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={() => undefined} onCancel={() => undefined} />);
    expect(document.querySelector('[data-field="allow_replace"]')).toBeNull();
    await user.click(screen.getByRole("switch", { name: "Expert" }));
    expect(document.querySelector('[data-field="allow_replace"]')).not.toBeNull();
  });

  it("a grant's picked element writes its type (the `auto` field) and its name in the sentence", async () => {
    const user = userEvent.setup();
    const onOk = vi.fn();
    const s = spec("grant");
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={onOk} onCancel={() => undefined} />);
    await user.click(screen.getByRole("button", { name: "Pick: What it gives" }));
    await user.dblClick(await within(await screen.findByRole("list", { name: "What it gives options" })).findByRole("button", { name: /^Stealth/ }));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onOk.mock.lastCall![0]).toMatchObject({ kind: "grant", fields: { id: "skill-stealth", type: "skill" }, sentence: "Gives ... (Stealth)" });
  });

  it("a variant switches shape (a pool's maximum: a fixed number -> a formula)", async () => {
    const user = userEvent.setup();
    const onOk = vi.fn();
    const s = spec("resource");
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={onOk} onCancel={() => undefined} />);
    await user.type(screen.getByLabelText(/^Pool name/), "Embers");
    await user.selectOptions(screen.getByRole("combobox", { name: "Maximum: kind" }), "formula");
    await user.click(screen.getByRole("button", { name: "N × class level" }));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onOk.mock.lastCall![0].fields).toMatchObject({ id: "Embers", max: { type: "formula", expr: { value: { shape: "model" } } } });
  });

  it("a written list numbers its rows 1..n automatically (never typed)", async () => {
    const user = userEvent.setup();
    const onOk = vi.fn();
    const s = spec("list");
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={onOk} onCancel={() => undefined} />);
    await user.type(screen.getByLabelText(/^Label/), "Ideal");
    await user.click(screen.getByRole("button", { name: "Add a row" }));
    await user.click(screen.getByRole("button", { name: "Add a row" }));
    const texts = screen.getAllByLabelText(/^Text/);
    await user.type(texts[0]!, "Charity");
    await user.type(texts[1]!, "Honor");
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onOk.mock.lastCall![0].fields).toMatchObject({ type: "List", name: "Ideal", items: [{ id: "1", text: "Charity" }, { id: "2", text: "Honor" }] });
  });

  it("parts kept from the original are read-only and removable only", async () => {
    const user = userEvent.setup();
    const onOk = vi.fn();
    const s = spec("stat");
    const kept = block("stat", { name: "speed", value: "10" }, { preserved: [{ key: "merge", label: "A setting Borealis does not use (“merge”)" }] });
    await renderControl(<BlockDialog spec={s} block={kept} isNew={false} problems={[]} onOk={onOk} onCancel={() => undefined} />);
    const section = screen.getByRole("region", { name: "Kept from the original" });
    expect(within(section).queryByRole("textbox")).toBeNull();
    expect(within(section).queryByRole("button", { name: /add/i })).toBeNull();
    await user.click(within(section).getByRole("button", { name: /^Remove A setting/ }));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onOk.mock.lastCall![0].preserved).toEqual([]);
  });

  it("a toggle's effects are a nested list offering only the effect kinds", async () => {
    const user = userEvent.setup();
    const s = spec("toggle");
    await renderControl(<BlockDialog spec={s} block={newBlock(s)} isNew problems={[]} onOk={() => undefined} onCancel={() => undefined} />);
    await user.click(within(document.querySelector<HTMLElement>('[data-field="effects"]')!).getByRole("button", { name: "Add rule" }));
    const menu = await screen.findByRole("dialog", { name: "Add a rule" });
    const offered = within(menu).getAllByRole("button").map((b) => b.textContent).filter((t) => t && t !== "Cancel" && !/^[A-Z ]+$/.test(t));
    expect(offered).toEqual(expect.arrayContaining(["Gives ...", "Changes a number", "Advantage on a roll", "Multiply a speed"]));
    expect(offered).toHaveLength(4);
  });
});

describe("WB-10 form parts kept from the original (spec 3.4)", () => {
  it("lists each part in words with Remove; nothing can be added or edited", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn();
    await renderControl(<PreservedList parts={[{ key: "data.pack_size", label: "Stored detail “pack size”" }]} onRemove={onRemove} />);
    const section = screen.getByRole("region", { name: "Kept from the original" });
    expect(within(section).getAllByRole("button")).toHaveLength(1);
    await user.click(within(section).getByRole("button", { name: "Remove Stored detail “pack size”" }));
    expect(onRemove).toHaveBeenCalledWith("data.pack_size");
  });
});

describe("WB-10 block values", () => {
  it("drops unset values, finds missing required ones, numbers rows", async () => {
    expect(cleanFields({ a: "", b: [], c: 0, d: false, e: { x: "" }, f: "x" })).toEqual({ c: 0, d: false, f: "x" });
    expect(missingRequired(spec("sense").fields, {}).map((f) => f.key)).toEqual(["sense"]);
    expect(numberRows(spec("list").fields.find((f) => f.key === "items")!, [{ text: "a" }, { text: "b" }])).toEqual([{ text: "a", id: "1" }, { text: "b", id: "2" }]);
    expect(newBlock(spec("list")).fields).toEqual({ type: "List", items: [] });
  });
});
