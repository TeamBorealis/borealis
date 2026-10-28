import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption, SlotOptionBase } from "../api/contract";
import { MockApi } from "../api/mock";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { renderApp } from "./utils";

/** B-55 / B-54 picker filter bar (specs/version-rows-and-filters.md sec. 1.5). */

type V = SlotOptionBase;
const ver = (id: string, name: string, sourceName: string, rulesets: V["rulesets"], extra: Partial<V> = {}): V => ({
  id, name, type: "race", source: sourceName.toLowerCase().replace(/\W+/g, "-"), sourceName, shortDescription: "", prerequisites: null, rulesets, ...extra,
});
const row = (...versions: V[]): SlotOption => (versions.length > 1 ? { ...versions[0]!, versions } : { ...versions[0]! });

const ROWS = [
  row(ver("aasimar", "Aasimar", "Player Guide (2024)", ["dnd2024"])),
  row(ver("db-exp", "Dragonborn", "Sample Expansion", ["dnd2024"]), ver("db-24", "Dragonborn", "Sample Reference 5.2", ["dnd2024"], { bundled: true }),
    ver("db-14", "Dragonborn", "Sample Reference 5.1", ["dnd2014"], { bundled: true, edition: "dnd2014" })),
  row(ver("half-elf", "Half-Elf", "Sample Reference 5.1", ["dnd2014"], { edition: "dnd2014" })),
  row(ver("human", "Human", "Sample Reference 5.1", ["dnd2014", "dnd2024"])),
];

const slotWith = (chosen: SelectionSlot["chosen"] = null) =>
  ({ id: "s1", title: "Species", groupLabel: null, locked: false, chosen, elementType: "race", hint: null, requiredLevel: null }) as unknown as SelectionSlot;

function renderPicker(rows: SlotOption[], slot = slotWith()) {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (): Promise<Page<SlotOption>> => ({ total: rows.length, items: rows });
  render(
    <AppProviders api={api}>
      <MemoryRouter initialEntries={["/builder/race"]}>
        <Routes>
          <Route path="/builder/race" element={<PickerGroup characterId="c1" group={{ key: "s1", label: null, title: "Species", slots: [slot] }} isNext={false} open
            onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />} />
          <Route path="/content/sources" element={<p>Sources page</p>} />
        </Routes>
      </MemoryRouter>
    </AppProviders>,
  );
}

const names = async () => {
  const list = await screen.findByRole("list", { name: "Species options" });
  return within(list).queryAllByRole("listitem").map((li) => li.querySelector(".pick-name")!.textContent);
};
const bar = () => screen.findByRole("search", { name: "Filter Species" });

describe("filter bar visibility", () => {
  it("is hidden on a short single-source list", async () => {
    renderPicker([row(ver("a", "Alpha", "One Book", ["dnd2024"])), row(ver("b", "Beta", "One Book", ["dnd2024"]))]);
    await waitFor(async () => expect(await names()).toEqual(["Alpha", "Beta"]));
    expect(screen.queryByRole("search")).toBeNull();
  });

  it("shows on more than 8 rows", async () => {
    renderPicker(Array.from({ length: 9 }, (_, i) => row(ver(`r${i}`, `Row ${i}`, "One Book", ["dnd2024"]))));
    expect(await bar()).toBeTruthy();
  });

  it("shows on more than one source", async () => {
    renderPicker([row(ver("a", "Alpha", "One Book", ["dnd2024"])), row(ver("b", "Beta", "Two Book", ["dnd2024"]))]);
    expect(await bar()).toBeTruthy();
  });
});

describe("filters", () => {
  it("search matches the row name; the count says how many of how many", async () => {
    renderPicker(ROWS);
    const user = userEvent.setup();
    const b = await bar();
    expect(within(b).getByRole("status").textContent).toBe("4 entries");
    await user.type(within(b).getByRole("searchbox", { name: "Search species" }), "  DRAGON");
    expect(await names()).toEqual(["Dragonborn"]);
    expect(within(b).getByRole("status").textContent).toBe("1 of 4 entries");
  });

  it("Edition keeps versions of that edition plus edition-neutral ones and recomputes the shown version", async () => {
    renderPicker(ROWS);
    const user = userEvent.setup();
    const b = await bar();
    await user.click(within(within(b).getByRole("group", { name: "Edition" })).getByRole("button", { name: "2014" }));
    expect(await names()).toEqual(["Dragonborn", "Half-Elf", "Human"]);
    const list = screen.getByRole("list", { name: "Species options" });
    expect(within(list).getByRole("button", { name: "Dragonborn, Sample Reference 5.1, 2014 rules" })).toBeTruthy();
    // One passing version left: no version column, no menu.
    expect(list.querySelector(".pick-ver")).toBeNull();
    await user.click(within(b).getByRole("button", { name: "2024" }));
    expect(await names()).toEqual(["Aasimar", "Dragonborn", "Human"]);
    expect(within(list).getByRole("button", { name: "2 versions of Dragonborn" })).toBeTruthy();
  });

  it("Source checklist: sources of this list with their edition, unticking narrows the versions, Manage Sources link", async () => {
    renderPicker(ROWS);
    const user = userEvent.setup();
    const b = await bar();
    const src = within(b).getByRole("button", { name: "Source: All sources" });
    await user.click(src);
    const menu = await screen.findByRole("dialog", { name: "Sources" });
    const labels = within(menu).getAllByRole("checkbox").map((c) => c.closest("label")!.textContent);
    expect(labels).toEqual(["Player Guide (2024)2024", "Sample Expansion2024", "Sample Reference 5.1Any edition", "Sample Reference 5.22024"]);
    await user.click(within(menu).getByRole("checkbox", { name: /Sample Expansion/ }));
    expect(within(b).getByRole("button", { name: "Source: 3 of 4 sources" })).toBeTruthy();
    const list = screen.getByRole("list", { name: "Species options" });
    expect(within(list).getByRole("button", { name: "Dragonborn, Sample Reference 5.2, 2 versions" })).toBeTruthy();
    await user.click(within(menu).getByRole("checkbox", { name: /Sample Expansion/ }));
    expect(within(b).getByRole("button", { name: "Source: All sources" })).toBeTruthy();
    await user.click(within(menu).getByRole("button", { name: "Manage Sources..." }));
    expect(await screen.findByText("Sources page")).toBeTruthy();
  });

  it("Clear resets all three filters", async () => {
    renderPicker(ROWS);
    const user = userEvent.setup();
    const b = await bar();
    await user.type(within(b).getByRole("searchbox"), "h");
    await user.click(within(b).getByRole("button", { name: "2014" }));
    expect(await names()).toEqual(["Half-Elf", "Human"]);
    await user.click(within(b).getByRole("button", { name: "Clear" }));
    expect(await names()).toEqual(["Aasimar", "Dragonborn", "Half-Elf", "Human"]);
    expect((within(b).getByRole("searchbox") as HTMLInputElement).value).toBe("");
    expect(within(b).getByRole("button", { name: "All" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("the chosen row always stays visible, with its chosen version", async () => {
    renderPicker(ROWS, slotWith({ id: "aasimar", name: "Aasimar", type: "race", source: "player-guide-2024-" }));
    const user = userEvent.setup();
    const b = await bar();
    await user.click(within(b).getByRole("button", { name: "2014" }));
    await user.type(within(b).getByRole("searchbox"), "half");
    expect(await names()).toEqual(["Aasimar", "Half-Elf"]);
  });

  it("empty result: 'Nothing matches these filters.' with Clear", async () => {
    renderPicker(ROWS);
    const user = userEvent.setup();
    const b = await bar();
    await user.type(within(b).getByRole("searchbox"), "zzz");
    expect(await names()).toEqual([]);
    const empty = await screen.findByText(/Nothing matches these filters\./);
    expect(screen.queryByText("Nothing matches. Check your source restrictions.")).toBeNull();
    await user.click(within(empty).getByRole("button", { name: "Clear" }));
    expect(await names()).toHaveLength(4);
  });
});

describe("filter state lives while the builder is open", () => {
  it("survives a switch to another builder tab and back", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const snap = await api.createCharacter({ request: { name: "Keep", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    renderApp("/builder/race", { api, openCharacterId: snap.id });
    const user = userEvent.setup();
    const b = await screen.findByRole("search", { name: "Filter Species" });
    await user.type(within(b).getByRole("searchbox"), "dra");
    await user.click(screen.getByRole("tab", { name: "Class" }));
    await screen.findByRole("list", { name: "Class options" });
    await user.click(screen.getByRole("tab", { name: "Species" }));
    const again = await screen.findByRole("search", { name: "Filter Species" });
    expect((within(again).getByRole("searchbox") as HTMLInputElement).value).toBe("dra");
  });
});
