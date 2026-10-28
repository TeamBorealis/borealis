import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ClassSpellcasting, Page, SelectionSlot, SlotOption, SlotOptionBase, SlotOptionsArgs } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { fetchAllSlotOptions } from "../pages/builder/usePickerRows";
import { SlotOptionsGrid } from "../pages/spells/SlotOptionsGrid";
import { renderApp } from "./utils";

/** B-55 UI (specs/version-rows-and-filters.md sec. 1.2-1.4, 1.6, 2, 6): one row per name, version column and menu. */

afterEach(() => { setPhoneForTests(false); });

type V = SlotOptionBase;
const ver = (id: string, name: string, sourceName: string, rulesets: V["rulesets"], extra: Partial<V> = {}): V => ({
  id, name, type: "race", source: sourceName.toLowerCase().replace(/\W+/g, "-"), sourceName, shortDescription: `${name} from ${sourceName}.`,
  prerequisites: null, rulesets, ...extra,
});
/** A host row: its own fields are versions[0]. */
const row = (...versions: V[]): SlotOption => (versions.length > 1 ? { ...versions[0]!, versions } : { ...versions[0]! });

const DB_EXP = ver("db-exp", "Dragonborn", "Sample Expansion", ["dnd2024"]);
const DB_24 = ver("db-24", "Dragonborn", "Sample Reference 5.2", ["dnd2024"], { bundled: true });
const DB_14 = ver("db-14", "Dragonborn", "Sample Reference 5.1", ["dnd2014"], { bundled: true, edition: "dnd2014", editionNotes: ["used as written"] });
const AASIMAR = ver("aasimar", "Aasimar", "Player Guide (2024)", ["dnd2024"]);
const HALF_ELF = ver("half-elf", "Half-Elf", "Sample Reference 5.1", ["dnd2014"], { edition: "dnd2014" });
const OLD_BOOK = ver("old-book", "Old Kin", "Old Guide (2014)", ["dnd2014"], { edition: "dnd2014" });
const ROWS = [row(AASIMAR), row(DB_EXP, DB_24, DB_14), row(HALF_ELF), row(OLD_BOOK)];

const SLOT = { id: "s1", title: "Species", groupLabel: null, locked: false, chosen: null, elementType: "race", hint: null, requiredLevel: null } as unknown as SelectionSlot;

function apiWith(rows: SlotOption[]) {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (a: SlotOptionsArgs): Promise<Page<SlotOption>> => {
    const offset = a.offset ?? 0;
    return { total: rows.length, items: rows.slice(offset, offset + (a.limit ?? 200)) };
  };
  return api;
}

function renderPicker(rows: SlotOption[], opts: { slot?: SelectionSlot; onDescribe?: (id: string) => void; onChoose?: (s: string, e: string) => void } = {}) {
  const api = apiWith(rows);
  const slot = opts.slot ?? SLOT;
  render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: slot.id, label: null, title: slot.title, slots: [slot] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={(id) => opts.onDescribe?.(id)} onChoose={(s, e) => opts.onChoose?.(s, e)} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
  return api;
}

const items = async (title = "Species") => {
  const list = await screen.findByRole("list", { name: `${title} options` });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0));
  return within(list).getAllByRole("listitem");
};
const rowOf = (lis: HTMLElement[], name: string) => lis.find((li) => li.querySelector(".pick-name")?.textContent?.startsWith(name))!;

describe("one row per name", () => {
  it("draws one row per name; the row shows its default version", async () => {
    renderPicker(ROWS);
    const lis = await items();
    expect(lis.map((li) => li.querySelector(".pick-name")!.textContent)).toEqual(["Aasimar", "Dragonborn", "Half-Elf", "Old Kin"]);
    // Default: the character's edition first, imported before bundled.
    expect(rowOf(lis, "Dragonborn").querySelector(".pick-src-name")!.textContent).toBe("Sample Expansion");
  });

  it("source names end at the same x: every row has the version column, only multi-version rows fill it", async () => {
    renderPicker(ROWS);
    const lis = await items();
    for (const li of lis) expect(li.querySelector(":scope > .pick-ver")).not.toBeNull();
    expect(rowOf(lis, "Dragonborn").querySelector(".pick-ver button")).not.toBeNull();
    expect(rowOf(lis, "Dragonborn").querySelector(".pick-ver-count")!.textContent).toBe("3");
    expect(rowOf(lis, "Aasimar").querySelector(".pick-ver")!.childElementCount).toBe(0);
  });

  it("a list without versions has no version column", async () => {
    renderPicker([row(AASIMAR), row(HALF_ELF)]);
    const lis = await items();
    for (const li of lis) expect(li.querySelector(".pick-ver")).toBeNull();
  });

  it("the row button's accessible name: name, source, edition rules, version count", async () => {
    renderPicker(ROWS);
    const list = await screen.findByRole("list", { name: "Species options" });
    expect(await within(list).findByRole("button", { name: "Dragonborn, Sample Expansion, 3 versions" })).toBeTruthy();
    expect(within(list).getByRole("button", { name: "Half-Elf, Sample Reference 5.1, 2014 rules" })).toBeTruthy();
    expect(within(list).getByRole("button", { name: "3 versions of Dragonborn" })).toBeTruthy();
  });
});

describe("edition mark (sec. 1.3)", () => {
  it("writes ' (2014)' after an other-edition source, omits it when the source names the year, never a boxed badge", async () => {
    renderPicker(ROWS);
    const lis = await items();
    expect(rowOf(lis, "Half-Elf").querySelector(".pick-src-name")!.textContent).toBe("Sample Reference 5.1 (2014)");
    expect(rowOf(lis, "Half-Elf").querySelector(".ed-mark")!.getAttribute("title")).toBe("Written for the 2014 rules");
    expect(rowOf(lis, "Old Kin").querySelector(".pick-src-name")!.textContent).toBe("Old Guide (2014)");
    expect(rowOf(lis, "Aasimar").querySelector(".ed-mark")).toBeNull();
    expect(document.querySelector(".ed-badge")).toBeNull();
  });
});

describe("version menu (sec. 1.4)", () => {
  it("lists one section per edition, character's edition first, checks the shown version and marks disabled ones", async () => {
    const disabled = ver("db-off", "Dragonborn", "Zed Pack", ["dnd2024", "dnd2014"], { disabledReason: "Requires Zed Pack" });
    renderPicker([row(AASIMAR), row(DB_EXP, DB_24, DB_14, disabled)]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "4 versions of Dragonborn" }));
    const menu = await screen.findByRole("listbox", { name: "Versions of Dragonborn" });
    expect(within(menu).getAllByRole("group").map((g) => g.querySelector(".vm-head")!.textContent)).toEqual(["2024 rules (your character)", "2014 rules", "Any edition"]);
    const opts = within(menu).getAllByRole("option");
    expect(opts.map((o) => o.getAttribute("aria-label"))).toEqual(["Sample Expansion", "Sample Reference 5.2", "Sample Reference 5.1", "Zed Pack, Requires Zed Pack"]);
    expect(opts.map((o) => o.getAttribute("aria-selected"))).toEqual(["true", "false", "false", "false"]);
    expect(opts[3]!.getAttribute("aria-disabled")).toBe("true");
    await user.click(opts[3]!);
    expect(screen.getByRole("listbox", { name: "Versions of Dragonborn" })).toBeTruthy();
  });

  it("picking a version makes it the shown version: the row's source, the description and double-click follow it", async () => {
    const described: string[] = [];
    const chosen: string[] = [];
    renderPicker(ROWS, { onDescribe: (id) => described.push(id), onChoose: (_s, e) => chosen.push(e) });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "3 versions of Dragonborn" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "Sample Reference 5.1" }));
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(described.at(-1)).toBe("db-14");
    const lis = await items();
    expect(rowOf(lis, "Dragonborn").querySelector(".pick-src-name")!.textContent).toBe("Sample Reference 5.1 (2014)");
    fireEvent.doubleClick(rowOf(lis, "Dragonborn").querySelector("button.pick-main")!);
    expect(chosen).toEqual(["db-14"]);
    // The menu now checks the new shown version.
    await user.click(screen.getByRole("button", { name: "3 versions of Dragonborn" }));
    expect(within(await screen.findByRole("listbox")).getByRole("option", { selected: true }).getAttribute("aria-label")).toBe("Sample Reference 5.1");
  });

  it("the source name of a multi-version row opens the menu too", async () => {
    renderPicker(ROWS);
    const lis = await items();
    fireEvent.click(rowOf(lis, "Dragonborn").querySelector(".pick-src-name")!);
    expect(await screen.findByRole("listbox", { name: "Versions of Dragonborn" })).toBeTruthy();
  });

  it("keyboard: Enter on the count opens, Down moves, Enter picks; Esc closes and returns focus; a click outside closes", async () => {
    const described: string[] = [];
    renderPicker(ROWS, { onDescribe: (id) => described.push(id) });
    const user = userEvent.setup();
    const btn = await screen.findByRole("button", { name: "3 versions of Dragonborn" });
    btn.focus();
    await user.keyboard("{Enter}");
    const menu = await screen.findByRole("listbox");
    expect(document.activeElement).toBe(menu);
    expect(btn.getAttribute("aria-expanded")).toBe("true");
    await user.keyboard("{ArrowDown}{Enter}");
    expect(described.at(-1)).toBe("db-24");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(document.activeElement).toBe(btn);
    await user.keyboard(" ");
    await screen.findByRole("listbox");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(document.activeElement).toBe(btn);
    await user.click(btn);
    await screen.findByRole("listbox");
    fireEvent.mouseDown(document.body);
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  });
});

describe("version switch on the chosen row (sec. 1.4, 3)", () => {
  async function builderWithDragonborn(ancestry: string) {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const snap = await api.createCharacter({ request: { name: "Vers", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const race = snap.slots.find((s) => s.elementType === "race")!;
    const after = await api.applySelection({ characterId: snap.id, slotId: race.id, elementId: "dragonborn" });
    const sub = after.slots.find((s) => s.title === "Draconic Ancestry")!;
    await api.applySelection({ characterId: snap.id, slotId: sub.id, elementId: ancestry });
    const spy = vi.spyOn(api, "switchVersion");
    renderApp(`/builder/race?slot=${encodeURIComponent(race.id)}`, { api, openCharacterId: snap.id });
    return { api, spy, raceId: race.id };
  }

  it("calls switchVersion, keeps the pick chosen and shows the toast", async () => {
    const { spy, raceId } = await builderWithDragonborn("ancestry-red");
    const user = userEvent.setup();
    const btn = await screen.findByRole("button", { name: "3 versions of Dragonborn" });
    await user.click(btn);
    const menu = await screen.findByRole("listbox", { name: "Versions of Dragonborn" });
    expect(within(menu).getByRole("option", { selected: true }).getAttribute("aria-label")).toBe("Sample Reference 5.1, chosen");
    await user.click(within(menu).getByRole("option", { name: "Sample Reference 5.2" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ slotId: raceId, elementId: "dragonborn-2024" })));
    expect(await screen.findByText("Switched Dragonborn to Sample Reference 5.2 (2024 rules).")).toBeTruthy();
    await waitFor(() => expect(document.querySelector(`.pick-chosen[data-slot-id="${raceId}"] .pick-chosen-src`)!.textContent).toBe("Sample Reference 5.2"));
  });

  it("names the dropped sub-choices in the toast", async () => {
    await builderWithDragonborn("ancestry-blue");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "3 versions of Dragonborn" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "Sample Reference 5.2" }));
    expect(await screen.findByText("Switched Dragonborn to Sample Reference 5.2 (2024 rules). Choose again: Draconic Ancestry.")).toBeTruthy();
  });

  it("the chosen-pick footer writes the edition mark", async () => {
    const { raceId } = await builderWithDragonborn("ancestry-red");
    await waitFor(() => expect(document.querySelector(`.pick-chosen[data-slot-id="${raceId}"] .pick-chosen-src`)!.textContent).toBe("Sample Reference 5.1 (2014)"));
  });
});

describe("fetch-all (sec. 1.6)", () => {
  const many = Array.from({ length: 250 }, (_, i) => row(ver(`r${i}`, `Row ${String(i).padStart(3, "0")}`, "Big Pack", ["dnd2024"])));

  it("a slot with 250 rows lists 250", async () => {
    renderPicker(many);
    await waitFor(async () => expect(await items()).toHaveLength(250));
  });

  it("pages by offset whatever page size the host uses", async () => {
    for (const size of [1, 7, 200, 1000]) {
      const calls: SlotOptionsArgs[] = [];
      const host = { getSlotOptions: async (a: SlotOptionsArgs) => { calls.push(a); const o = a.offset ?? 0; return { total: many.length, items: many.slice(o, o + size) }; } };
      const page = await fetchAllSlotOptions(host, { characterId: "c", slotId: "s", query: "" });
      expect(page.items.map((r) => r.id)).toEqual(many.map((r) => r.id));
      expect(calls).toHaveLength(Math.ceil(many.length / size));
      expect(calls.every((c) => c.limit === undefined)).toBe(true);
    }
  });

  it("stops on an empty page even when total says more", async () => {
    const host = { getSlotOptions: async (a: SlotOptionsArgs) => ({ total: 10, items: (a.offset ?? 0) === 0 ? many.slice(0, 3) : [] }) };
    expect((await fetchAllSlotOptions(host, { characterId: "c", slotId: "s" })).items).toHaveLength(3);
  });
});

describe("phones (sec. 2)", () => {
  it("the version menu is a bottom sheet and the count is a 44 pt target", async () => {
    setPhoneForTests(true);
    renderPicker(ROWS);
    const user = userEvent.setup();
    const btn = await screen.findByRole("button", { name: "3 versions of Dragonborn" });
    expect(btn.closest(".pick-ver")).not.toBeNull();
    await user.click(btn);
    const sheet = await screen.findByRole("dialog", { name: "Dragonborn" });
    expect(sheet.closest(".flyout-overlay")?.getAttribute("data-sheet")).toBe("auto");
    expect(within(sheet).getByRole("listbox", { name: "Versions of Dragonborn" })).toBeTruthy();
    await user.click(within(sheet).getByRole("option", { name: "Sample Reference 5.2" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Dragonborn" })).toBeNull());
  });

  it("the Choose button chooses the shown version", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const snap = await api.createCharacter({ request: { name: "Ph", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const race = snap.slots.find((s) => s.elementType === "race")!;
    const spy = vi.spyOn(api, "applySelection");
    act(() => setPhoneForTests(true));
    renderApp("/builder/race", { api, openCharacterId: snap.id });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "3 versions of Dragonborn" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "Sample Reference 5.1" }));
    await user.click(await screen.findByRole("button", { name: "Choose Dragonborn" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ slotId: race.id, elementId: "dragonborn" })));
  });
});

describe("spell slot catalogue row model (sec. 1, 6)", () => {
  it("one row per name, version column, edition mark, and the shown version registers", async () => {
    const sp = (id: string, src: string, rulesets: V["rulesets"], extra: Partial<V> = {}) => ver(id, "Fireball", src, rulesets, { type: "spell", level: 3, school: "Evocation", ...extra });
    const spells = [row(sp("fb-14", "Sample Reference 5.1", ["dnd2014"]), sp("fb-24", "Sample Reference 5.2", ["dnd2024"], { edition: "dnd2024" })), row(ver("mm", "Magic Missile", "Sample Reference 5.1", ["dnd2014"], { type: "spell", level: 1 }))];
    const chosen: string[] = [];
    const picked: string[] = [];
    render(
      <AppProviders api={apiWith(spells)}>
        <MemoryRouter>
          <SlotOptionsGrid characterId="c1" cls={{ className: "Wizard" } as ClassSpellcasting} slot={{ ...SLOT, elementType: "spell" } as SelectionSlot} picked={null}
            onPick={(id) => picked.push(id)} onChoose={(_s, e) => chosen.push(e)} />
        </MemoryRouter>
      </AppProviders>,
    );
    const table = await screen.findByRole("table", { name: "Wizard spells" });
    await waitFor(() => expect(within(table).getAllByText("Fireball")).toHaveLength(1));
    const user = userEvent.setup();
    await user.click(within(table).getByRole("button", { name: "2 versions of Fireball" }));
    const menu = await screen.findByRole("listbox", { name: "Versions of Fireball" });
    expect(within(menu).getAllByRole("group").map((g) => g.querySelector(".vm-head")!.textContent)).toEqual(["2014 rules (your character)", "2024 rules"]);
    await user.click(within(menu).getByRole("option", { name: "Sample Reference 5.2" }));
    expect(picked.at(-1)).toBe("fb-24");
    const tr = within(table).getByText("Fireball").closest("tr")!;
    expect(tr.querySelector(".sp-src-name")!.textContent).toBe("Sample Reference 5.2 (2024)");
    expect(within(table).getByText("Magic Missile").closest("tr")!.querySelector(".sp-ver")!.childElementCount).toBe(0);
    fireEvent.doubleClick(tr);
    expect(chosen).toEqual(["fb-24"]);
  });
});
