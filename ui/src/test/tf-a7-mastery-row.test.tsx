import { render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { Page, SelectionSlot, SlotOption, SlotOptionBase } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { PickerGroup } from "../pages/builder/PickerGroup";
import { renderApp } from "./utils";

/**
 * A7 (tester video 2026-11-02) with TV2 (A12): a Weapon Mastery picker says what the pick is, numbered, and on phones each
 * row's second line names the weapon's mastery property (in place of a source every row shares).
 */
afterEach(() => setPhoneForTests(false));

const weapon = (id: string, name: string, mastery: string | null, sourceName = "SRD 5.2.1"): SlotOption => ({
  id, name, type: "weapon", source: sourceName, sourceName, shortDescription: "", prerequisites: null, rulesets: ["dnd2024"],
  weapon: { damage: "1d8", damageType: "slashing", properties: [], mastery, range: null, proficiency: "Martial", kind: "Melee" },
} as SlotOptionBase as SlotOption);

const MASTERY_SLOT = {
  id: "m1", title: "Weapon Mastery", groupLabel: null, locked: false, chosen: null, elementType: "weapon", requiredLevel: null,
  hint: "Choose a kind of weapon whose mastery property you can use (1 of 2)",
} as unknown as SelectionSlot;
const OTHER_WEAPON_SLOT = { ...MASTERY_SLOT, id: "w1", title: "Bonus Weapon", hint: null } as SelectionSlot;

function renderPicker(slot: SelectionSlot, rows: SlotOption[]) {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  api.presetSettings({ showImportNotice: false });
  api.getSlotOptions = async (): Promise<Page<SlotOption>> => ({ total: rows.length, items: rows });
  render(
    <AppProviders api={api}>
      <MemoryRouter>
        <PickerGroup characterId="c1" group={{ key: slot.id, label: null, title: slot.title, slots: [slot] }} isNext={false} open
          onToggle={() => {}} describedId={null} onDescribe={() => {}} onChoose={() => {}} onClear={() => {}} />
      </MemoryRouter>
    </AppProviders>,
  );
}

async function rowOf(title: string, name: string): Promise<HTMLElement> {
  const list = await screen.findByRole("list", { name: `${title} options` });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0));
  return within(list).getAllByRole("listitem").find((li) => li.getAttribute("data-row") === name)!;
}

const ROWS = [weapon("axe", "Battleaxe", "Topple"), weapon("sword", "Longsword", "Sap")];

describe("Weapon Mastery rows (A7)", () => {
  it("on phones the row's second line is the weapon's mastery when every row shares a source", async () => {
    setPhoneForTests(true);
    renderPicker(MASTERY_SLOT, ROWS);
    const axe = await rowOf("Weapon Mastery", "Battleaxe");
    expect(axe.querySelector(".pick-src")?.textContent).toBe("Topple");
    expect(within(axe).getByRole("button", { name: "Battleaxe, Topple mastery, SRD 5.2.1" })).toBeTruthy();
    expect(screen.getByText("Choose a kind of weapon whose mastery property you can use (1 of 2)")).toBeTruthy();
  });

  it("keeps the source beside the mastery when the rows' sources differ", async () => {
    setPhoneForTests(true);
    renderPicker(MASTERY_SLOT, [...ROWS, weapon("whip", "Whip", "Slow", "Sample Expansion")]);
    const whip = await rowOf("Weapon Mastery", "Whip");
    expect(whip.querySelector(".pick-src")?.textContent).toBe("Slow · Sample Expansion");
  });

  it("other weapon pickers show no mastery: the source line only (A12: when the rows' sources differ)", async () => {
    setPhoneForTests(true);
    renderPicker(OTHER_WEAPON_SLOT, [...ROWS, weapon("whip", "Whip", "Slow", "Sample Expansion")]);
    const axe = await rowOf("Bonus Weapon", "Battleaxe");
    expect(axe.querySelector(".pick-mastery")).toBeNull();
    expect(axe.querySelector(".pick-src")?.textContent).toBe("SRD 5.2.1");
  });

  it("desktop rows keep the source line", async () => {
    renderPicker(MASTERY_SLOT, ROWS);
    expect((await rowOf("Weapon Mastery", "Battleaxe")).querySelector(".pick-src")?.textContent).toBe("SRD 5.2.1");
  });

  it("the mock host numbers its mastery hints like the host", async () => {
    const api = new MockApi();
    const created = await api.createCharacter({ request: { name: "Ona", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const cls = created.slots.find((s) => s.elementType === "class")!;
    const snap = await api.applySelection({ characterId: created.id, slotId: cls.id, elementId: "fighter" });
    const hints = snap.slots.filter((s) => s.title === "Weapon Mastery").map((s) => s.hint);
    expect(hints.length).toBeGreaterThan(1);
    hints.forEach((h, i) => expect(h).toBe(`Choose a kind of weapon whose mastery property you can use (${i + 1} of ${hints.length})`));
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    expect((await screen.findAllByText(/^Choose a kind of weapon whose mastery property you can use \(1 of/)).length).toBe(1);
  });
});
