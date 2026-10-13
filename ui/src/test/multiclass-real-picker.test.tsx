import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { ElementDetail, Page, SlotOption } from "../api/contract";
import { renderApp, seededId } from "./utils";

/** What the real backend answers for a 2024 Wizard 5 (STR 13) "Multiclass (Level 6)" slot (src-tauri tests/multiclass_picker.rs). */
const REAL_FIGHTER = {
  id: "ID_2024_MULTICLASS_FIGHTER", name: "Fighter", type: "multiclass", source: "SRD 5.2.1", sourceName: "SRD 5.2.1",
  shortDescription: "A master of martial combat.", prerequisites: "Strength 13 or Dexterity 13", rulesets: ["dnd2024"],
} as unknown as SlotOption;
const REAL_BARBARIAN = { ...REAL_FIGHTER, id: "ID_2024_MULTICLASS_BARBARIAN", name: "Barbarian", prerequisites: "Strength 13" } as SlotOption;
const MOCK_ID: Record<string, string> = { [REAL_FIGHTER.id]: "multi-fighter", [REAL_BARBARIAN.id]: "multi-barbarian" };

/**
 * MockApi whose Multiclass slot speaks the real backend's ids (engine-derived `ID_2024_MULTICLASS_*` entries).
 * Regression: the real picker said "Nothing matches" because the backend dropped these entries.
 */
class RealMulticlassApi extends MockApi {
  readonly applied: Array<{ slotId: string; elementId: string }> = [];

  private async isMulticlassSlot(characterId: string, slotId: string): Promise<boolean> {
    const snap = await super.getSnapshot({ characterId });
    return snap.slots.find((s) => s.id === slotId)?.elementType === "multiclass";
  }

  override async getSlotOptions(args: Parameters<MockApi["getSlotOptions"]>[0]): Promise<Page<SlotOption>> {
    if (!(await this.isMulticlassSlot(args.characterId, args.slotId))) return super.getSlotOptions(args);
    return { total: 2, items: [REAL_BARBARIAN, REAL_FIGHTER] };
  }

  override async getElement({ elementId }: { elementId: string }): Promise<ElementDetail> {
    return super.getElement({ elementId: MOCK_ID[elementId] ?? elementId });
  }

  override async applySelection(args: Parameters<MockApi["applySelection"]>[0]) {
    this.applied.push({ slotId: args.slotId, elementId: args.elementId });
    return super.applySelection({ ...args, elementId: MOCK_ID[args.elementId] ?? args.elementId });
  }
}

describe("New multiclass on real-shaped data", () => {
  it("lands on Build > Class with an empty Multiclass picker that lists the backend's classes and picks Fighter", async () => {
    const user = userEvent.setup();
    const api = new RealMulticlassApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    await user.click(within(await within(dlg).findByLabelText("Fighter multiclass")).getByRole("button", { name: /New multiclass/ }));

    // Spec (ui-workflows.md, ui-inventory.md): NEW MULTICLASS adds an empty class slot; the class is picked here.
    const options = await screen.findByRole("list", { name: /Multiclass.* options/ });
    expect(screen.queryByText(/Nothing matches/)).toBeNull();
    const fighter = within(options).getByRole("button", { name: /^Fighter/ });
    expect(within(options).getByText(/Requires Strength 13 or Dexterity 13/)).toBeTruthy();
    expect(within(options).getByRole("button", { name: /^Barbarian/ })).toBeTruthy();

    await user.click(fighter);
    await new Promise((r) => setTimeout(r, 30));
    expect(screen.queryByText("That element does not exist.")).toBeNull();

    await user.dblClick(fighter);
    await waitFor(() => expect(api.applied.at(-1)?.elementId).toBe("ID_2024_MULTICLASS_FIGHTER"));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).level.classes.map((c) => c.name)).toEqual(["Wizard", "Fighter"]));
  });
});
