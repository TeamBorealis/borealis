import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewContentsRow, HomebrewFieldSpec, HomebrewPackageOption } from "../api/contract";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { FieldView } from "../pages/homebrew/FieldView";
import { ContentsField, nextLetter, PackageOptionsField } from "../pages/homebrew/fields/ItemParts";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";

// Homebrew WB-12: the item forms (spec 4.9) on the web mock: the six kinds as borealis-authoring describes them, the
// weapon's mastery picker, pack contents and starting-equipment options, and a weapon and a magic item saved to a pack.

const fieldOf = (kind: string, key: string): HomebrewFieldSpec => {
  const f = MOCK_KIND_DESCRIPTORS.find((k) => k.id === kind)?.fields.find((x) => x.key === key);
  if (!f) throw new Error(`no field ${kind}.${key}`);
  return f;
};

describe("WB-12 item kinds (spec 4.9)", () => {
  it("the mock serves the six item forms with their pickers and ranges", async () => {
    // Arrange
    const api = mockApi();
    // Act
    const kinds = await api.homebrewKinds();
    // Assert
    const ids = kinds.map((k) => k.id);
    for (const id of ["item", "weapon", "armor", "magicItem", "itemPack", "equipmentPackage"]) expect(ids).toContain(id);
    expect(fieldOf("weapon", "mastery")).toMatchObject({ control: "references", picker: { types: ["Mastery Property"], multiple: true } });
    expect(fieldOf("armor", "baseAc")).toMatchObject({ control: "number", min: 0, max: 30 });
    expect(fieldOf("magicItem", "appliesToWeapon").control).toBe("tagFilter");
    expect(fieldOf("itemPack", "contents").control).toBe("contents");
    expect(fieldOf("equipmentPackage", "options").control).toBe("packageOptions");
  });

  it("the weapon's mastery field picks a mastery property by name; the id never shows", async () => {
    // Arrange
    const user = userEvent.setup();
    const onValue = vi.fn();
    const field = fieldOf("weapon", "mastery");
    await renderControl(
      <Stateful initial={[] as unknown} onValue={onValue}>{(v, set) => <FieldView field={field} value={v} problems={[]} onChange={(x) => set(x)} />}</Stateful>,
    );
    // Act
    await user.click(screen.getByRole("button", { name: "Pick: Mastery" }));
    const dialog = await screen.findByRole("dialog", { name: /^Choose/ });
    const list = await within(dialog).findByRole("list", { name: "Mastery options" });
    await user.dblClick(await within(list).findByRole("button", { name: /^Sap/ }));
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    // Assert
    await waitFor(() => expect(onValue).toHaveBeenLastCalledWith([expect.objectContaining({ id: "ID_2024_MASTERY_SAP", name: "Sap", type: "Mastery Property" })]));
    expect(document.body.textContent).not.toContain("ID_2024_MASTERY_SAP");
  });

  it("pack contents add a row, count it and remove it", async () => {
    // Arrange
    const user = userEvent.setup();
    const onValue = vi.fn();
    const rope: HomebrewContentsRow = { item: { id: "rope", name: "Rope", type: "item" }, amount: 1 };
    await renderControl(
      <Stateful initial={[rope]} onValue={onValue}>{(v, set) => <ContentsField label="Items in the pack" value={v} onChange={set} />}</Stateful>,
    );
    // Act
    await user.click(screen.getByRole("button", { name: "Add an item" }));
    await user.click(screen.getByRole("button", { name: "Increase Item 1: how many" }));
    // Assert
    expect(onValue).toHaveBeenNthCalledWith(1, [rope, { item: null, amount: 1 }]);
    expect(onValue).toHaveBeenLastCalledWith([{ ...rope, amount: 2 }, { item: null, amount: 1 }]);
    // Act
    await user.click(screen.getByRole("button", { name: "Remove item 2" }));
    // Assert
    expect(onValue).toHaveBeenLastCalledWith([{ ...rope, amount: 2 }]);
  });

  it("starting equipment options get the next letter, items and coins", async () => {
    // Arrange
    const user = userEvent.setup();
    const onValue = vi.fn();
    await renderControl(
      <Stateful initial={[] as HomebrewPackageOption[]} onValue={onValue}>{(v, set) => <PackageOptionsField label="Options" value={v} onChange={(o) => set(o)} />}</Stateful>,
    );
    // Act
    await user.click(screen.getByRole("button", { name: "Add an option" }));
    await user.click(screen.getByRole("button", { name: "Add an option" }));
    const b = screen.getByRole("group", { name: "Option B" });
    await user.click(within(b).getByRole("button", { name: "Add an item" }));
    await user.type(within(b).getByRole("textbox", { name: "Option B, item 1: or name a generic item" }), "A trinket");
    // Assert
    const last = onValue.mock.lastCall![0] as HomebrewPackageOption[];
    expect(last.map((o) => o.label)).toEqual(["A", "B"]);
    expect(last[1]!.items).toEqual([{ item: null, name: "A trinket", qty: 1, note: "" }]);
    expect(last[1]!.coins).toEqual({ cp: 0, sp: 0, ep: 0, gp: 0, pp: 0 });
    expect(nextLetter([{ label: "A", items: [], coins: last[0]!.coins }, { label: "c", items: [], coins: last[0]!.coins }])).toBe("B");
  });

  it("a weapon with a mastery and a magic item with a bonus save to the pack", async () => {
    // Arrange
    const api = mockApi();
    const pack = await api.homebrewCreatePack({ request: { name: "Forge", code: "FRG", edition: "dnd2024" } });
    const sap = { id: "ID_2024_MASTERY_SAP", name: "Sap", type: "Mastery Property" };
    // Act
    const w = await api.homebrewNewDraft({ packId: pack.id, kind: "weapon" });
    await api.homebrewUpdateDraft({ draftId: w.draftId, form: { ...w.form, fields: { ...w.form.fields, name: "Sickle Sword", damage: "1d6", mastery: [sap] } } });
    await api.homebrewSaveDraft({ draftId: w.draftId });
    const m = await api.homebrewNewDraft({ packId: pack.id, kind: "magicItem" });
    const view = await api.homebrewUpdateDraft({ draftId: m.draftId, form: { ...m.form, fields: { ...m.form.fields, name: "Ember Blade", rarity: "Rare", enhancement: 1 } } });
    await api.homebrewSaveDraft({ draftId: m.draftId });
    // Assert
    expect(view.preview?.facts).toEqual(expect.arrayContaining([{ label: "Rarity", value: "Rare" }, { label: "Bonus", value: "+1" }]));
    const list = await api.homebrewListElements({ packId: pack.id });
    expect(list.map((e) => [e.name, e.elementType])).toEqual([["Sickle Sword", "Weapon"], ["Ember Blade", "Magic Item"]]);
  });
});
