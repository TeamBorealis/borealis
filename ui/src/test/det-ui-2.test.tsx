import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MigrationIssue, RolledCharges } from "../api/contract";
import { MockApi } from "../api/mock";
import { ImportIssues, groupByFile, issueWhere, NO_FILE } from "../pages/content/ImportIssues";
import { chargesLabel, chargesSentence } from "../pages/equipment/RolledCharges";
import { closeImportPreview } from "../state/importPreview";
import { renderApp, seededId } from "./utils";

const NECKLACE = "necklace-of-beads";
const RESOURCE = "Necklace Of Beads charges";

/** A MockApi whose dice rolls follow `rolls` (0..1 each), then 0.5. */
function mockWithRolls(rolls: number[] = []) {
  const queue = [...rolls];
  return new MockApi({ rng: () => queue.shift() ?? 0.5 });
}

async function withNecklace(equipped = true, rolls: number[] = []) {
  const api = mockWithRolls(rolls);
  const id = await seededId(api, "Brannoc");
  let snap = await api.addItem({ characterId: id, itemId: NECKLACE, quantity: 1, buy: false });
  const line = snap.inventory.items.find((i) => i.itemId === NECKLACE)!;
  if (equipped) snap = await api.equipItem({ characterId: id, inventoryId: line.id, slot: "auto" });
  return { api, id, line, snap };
}

const issue = (over: Partial<MigrationIssue>): MigrationIssue => ({
  severity: "warning", code: "dangling_reference", message: "m", element: null, elementType: null, file: null, line: null, ...over,
});
const charge = (over: Partial<RolledCharges> = {}): RolledCharges => ({
  resourceId: NECKLACE, name: RESOURCE, dice: "1d6 + 3", value: 7, rolled: true, min: 4, max: 9, ...over,
});

describe("rolled item charges: MockApi (engine set_rolled_max / reroll_max)", () => {
  it("rolls the charges once when the item is gained and exposes them while it is equipped", async () => {
    // Arrange / Act: 1d6 with rng 0.5 -> 4, + 3.
    const { api, id, line, snap } = await withNecklace(false);
    expect(snap.inventory.items.find((i) => i.id === line.id)!.rolledCharges).toBeUndefined();
    const worn = await api.equipItem({ characterId: id, inventoryId: line.id, slot: "auto" });

    // Assert
    expect(worn.inventory.items.find((i) => i.id === line.id)!.rolledCharges).toEqual([charge()]);
    expect(worn.play.resources.find((r) => r.id === NECKLACE)).toMatchObject({ name: RESOURCE, max: 7, current: 7 });
  });

  it("enters a value inside min..max, refuses one outside it, and re-rolls", async () => {
    // Arrange
    const { api, id, line } = await withNecklace(true, [0.5, 0.99]);
    const args = { characterId: id, inventoryId: line.id, resourceId: NECKLACE };
    const valueOf = (s: Awaited<ReturnType<MockApi["getSnapshot"]>>) => s.inventory.items.find((i) => i.id === line.id)!.rolledCharges![0]!.value;

    // Act / Assert
    expect(valueOf(await api.setRolledMax({ ...args, value: 9 }))).toBe(9);
    await expect(api.setRolledMax({ ...args, value: 10 })).rejects.toMatchObject({ code: "invalid_argument", message: `10 is outside 4..9 for ${NECKLACE}` });
    await expect(api.setRolledMax({ ...args, value: 3 })).rejects.toMatchObject({ code: "invalid_argument" });
    expect(valueOf(await api.getSnapshot({ characterId: id }))).toBe(9);
    expect(valueOf(await api.rerollMax(args))).toBe(9); // the queued 0.99 -> 6 + 3
    expect(valueOf(await api.rerollMax(args))).toBe(7); // then 0.5 -> 4 + 3
  });

  it("refuses an item without dice-valued charges", async () => {
    const api = mockWithRolls();
    const id = await seededId(api, "Brannoc");
    const snap = await api.addItem({ characterId: id, itemId: "rope", quantity: 1, buy: false });
    const rope = snap.inventory.items.find((i) => i.itemId === "rope")!;
    await expect(api.rerollMax({ characterId: id, inventoryId: rope.id, resourceId: "rope" })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});

describe("rolled item charges: labels", () => {
  it("names the dice, and says when only the average is shown", () => {
    expect(chargesLabel(charge())).toBe("7 (1d6 + 3)");
    expect(chargesLabel(charge({ value: 6, rolled: false }))).toBe("6 (1d6 + 3, average)");
    expect(chargesSentence(charge())).toBe("Maximum 7, rolled on 1d6 + 3 (4 to 9).");
    expect(chargesSentence(charge({ value: 6, rolled: false }))).toBe("Maximum 6, the average of 1d6 + 3 (4 to 9). No roll is stored yet.");
  });
});

describe("rolled item charges: inventory and Play", () => {
  it("shows the rolled maximum on the inventory line; the details enter a value, show the refusal and re-roll", async () => {
    // Arrange
    const user = userEvent.setup();
    const { api, id } = await withNecklace(true, [0.5, 0]);
    renderApp("/equipment", { api, openCharacterId: id });
    const inv = await screen.findByRole("region", { name: "Inventory" });
    const row = () => within(inv).getByRole("button", { name: /Select Necklace Of Beads/ }).closest("tr")!;
    await within(inv).findByRole("button", { name: /Select Necklace Of Beads/ });
    expect(within(row()).getByText("Charges: 7 (1d6 + 3)")).toBeTruthy();

    // Act: open the details and enter a value outside 4..9.
    await user.dblClick(within(row()).getByRole("button", { name: /Select Necklace Of Beads/ }));
    const dialog = await screen.findByRole("dialog");
    const group = within(dialog).getByRole("group", { name: RESOURCE });
    const field = within(group).getByLabelText(`Maximum ${RESOURCE}`);
    await user.clear(field);
    await user.type(field, "12");
    await user.click(within(group).getByRole("button", { name: "Set maximum" }));

    // Assert: the backend's refusal under the field; nothing stored.
    expect(await within(group).findByText(`12 is outside 4..9 for ${NECKLACE}`)).toBeTruthy();
    expect(within(row()).getByText("Charges: 7 (1d6 + 3)")).toBeTruthy();

    // Act: a valid value, then a re-roll (rng 0 -> 1 + 3).
    await user.clear(field);
    await user.type(field, "5");
    await user.click(within(group).getByRole("button", { name: "Set maximum" }));
    await waitFor(() => expect(within(row()).getByText("Charges: 5 (1d6 + 3)")).toBeTruthy());
    expect(within(group).queryByText(/is outside/)).toBeNull();
    await user.click(within(group).getByRole("button", { name: "Re-roll" }));
    await waitFor(() => expect(within(row()).getByText("Charges: 4 (1d6 + 3)")).toBeTruthy());
    expect(within(group).getByText("Maximum 4, rolled on 1d6 + 3 (4 to 9).")).toBeTruthy();
  });

  it("the Play resource line names the rolled maximum and its dice; its details set the maximum", async () => {
    // Arrange
    const user = userEvent.setup();
    const { api, id } = await withNecklace();
    renderApp("/play", { api, openCharacterId: id });
    const list = await screen.findByRole("list", { name: "Resources" });
    const line = within(list).getByText(RESOURCE).closest("li")!;
    expect(within(line).getByText("Necklace Of Beads · max 7 (1d6 + 3)")).toBeTruthy();

    // Act
    await user.click(within(line).getByRole("button", { name: new RegExp(`^${RESOURCE}`) }));
    const field = await screen.findByLabelText(`Maximum ${RESOURCE}`);
    fireEvent.change(field, { target: { value: "8" } });
    await user.click(screen.getByRole("button", { name: "Set maximum" }));

    // Assert
    await waitFor(() => expect(within(line).getByRole("status", { name: `${RESOURCE}: 8 of 8 uses left` })).toBeTruthy());
  });
});

describe("+N ammunition and attunement read the DTO fields", () => {
  it("an auto attack firing carried +1 arrows names them (Attack.ammunition) and adds the bonus", async () => {
    // Arrange
    const api = mockWithRolls();
    const id = await seededId(api, "Brannoc");
    await api.addItem({ characterId: id, itemId: "shortbow", quantity: 1, buy: false });
    const before = await api.upsertAttack({ characterId: id, attack: { name: "Shortbow", range: "80/320", attackBonus: "", damage: "1d6", description: "piercing", ability: "dex", auto: true, showInCombat: true, card: false } });
    const bonus = (s: typeof before) => Number(s.attacks.find((a) => a.name === "Shortbow")!.attackBonus);
    expect(before.attacks.find((a) => a.name === "Shortbow")!.ammunition).toBeUndefined();

    // Act
    const after = await api.addItem({ characterId: id, itemId: "magic-arrows", quantity: 1, buy: false });

    // Assert
    expect(after.attacks.find((a) => a.name === "Shortbow")!.ammunition).toBe("Magic Arrows");
    expect(bonus(after)).toBe(bonus(before) + 1);
    const user = userEvent.setup();
    renderApp("/manage", { api, openCharacterId: id });
    await user.click(await screen.findByRole("tab", { name: "Attacks & Spellcasting" }));
    expect(await screen.findByText("Ammunition: Magic Arrows")).toBeTruthy();
  });

  it("the barred restriction comes from InventoryItem.attunementRestriction / attunementBarred", async () => {
    const api = mockWithRolls();
    const id = await seededId(api, "Brannoc");
    const snap = await api.addItem({ characterId: id, itemId: "staff-of-healing", quantity: 1, buy: false });
    expect(snap.inventory.items.find((i) => i.itemId === "staff-of-healing")).toMatchObject({ attunementRestriction: "by a bard, cleric, or druid", attunementBarred: true });
  });
});

describe("precise import reports", () => {
  beforeEach(() => closeImportPreview());

  it("names where an issue is and groups issues by file in path order, each by line", () => {
    const list = [
      issue({ file: "z/b.xml", line: 9, element: "ID_B", elementType: "Spell", code: "element_dropped" }),
      issue({ file: "a.xml", line: 30 }), issue({ file: "a.xml", line: 2 }), issue({ code: "index_entry_missing" }),
    ];
    expect(issueWhere(list[0]!)).toBe("Spell ID_B, line 9 · element_dropped");
    expect(issueWhere(issue({ line: 4 }))).toBe("line 4 · dangling_reference");
    expect(groupByFile(list).map(([f, rows]) => [f, rows.map((r) => r.line)])).toEqual([[NO_FILE, [null]], ["a.xml", [2, 30]], ["z/b.xml", [9]]]);
  });

  it("lists at most 500 issues per severity and says how many there are", async () => {
    const user = userEvent.setup();
    render(<ImportIssues issues={Array.from({ length: 501 }, (_, n) => issue({ file: "a.xml", line: n + 1 }))} />);
    await user.click(screen.getByRole("button", { name: "Warnings (501)" }));
    expect(screen.getByText("Only the first 500 of 501 are listed.")).toBeTruthy();
  });

  it("the preview and the install report show file path, element type and id, line and code", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mockWithRolls();
    renderApp("/content", { api });
    await screen.findByRole("button", { name: "Import files" });
    const text = `<elements>\n<element name="Fire" type="Spell" source="T" id="ID_T_FIRE">\n<spellcasting/>\n</element>\n</elements>`;
    fireEvent.change(screen.getByLabelText("Choose content files"), { target: { files: [new File([text], "spells/fire.xml")] } });
    const preview = await screen.findByRole("dialog", { name: /Import preview/ });

    // Act
    await user.click(within(preview).getByRole("button", { name: "Warnings (1)" }));

    // Assert: the preview row.
    const warnings = within(preview).getByRole("list", { name: "Warnings" });
    expect(within(warnings).getByLabelText("File spells/fire.xml")).toBeTruthy();
    expect(warnings.textContent).toContain("Spellcasting blocks were converted with default progression. (Spell ID_T_FIRE, line 3 · unsupported_construct)");

    // Act: install -> the install report opens with the same issues.
    await user.click(within(preview).getByRole("button", { name: "Install" }));
    const report = await screen.findByRole("dialog", { name: "Import report" });
    expect(within(report).getByText("1 element converted; 0 errors, 1 warning, 0 notes.")).toBeTruthy();
    await user.click(within(report).getByRole("button", { name: "Warnings (1)" }));
    expect(within(report).getByRole("list", { name: "Warnings" }).textContent).toContain("(Spell ID_T_FIRE, line 3 · unsupported_construct)");
    await user.click(within(report).getAllByRole("button", { name: "Close" }).at(-1)!);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Import report" })).toBeNull());
  });

  it("a clean install shows no report", async () => {
    const user = userEvent.setup();
    renderApp("/content", { api: mockWithRolls() });
    await screen.findByRole("button", { name: "Import files" });
    fireEvent.change(screen.getByLabelText("Choose content files"), { target: { files: [new File(['<elements><element name="A" type="Feat" source="T" id="ID_A"/></elements>'], "a.xml")] } });
    const preview = await screen.findByRole("dialog", { name: /Import preview/ });
    await user.click(within(preview).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull());
    expect(screen.queryByRole("dialog", { name: "Import report" })).toBeNull();
  });

  it("Update indexes returns one report per converted pack and shows it like a file import's report", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mockWithRolls();
    await api.addIndex({ url: "https://example.com/core.index" });
    const direct = await api.updateIndexes();
    expect(direct.reports).toHaveLength(1);
    expect(direct.reports[0]!.issues[0]).toMatchObject({ code: "dangling_reference", element: "ID_MOCK_CORE_FEAT", elementType: "Feat", file: "core/feats.xml", line: 12 });
    expect((await api.updateIndexes()).reports).toEqual([]); // up to date
    await api.addIndex({ url: "https://example.com/core.index" }); // updates available again
    renderApp("/content", { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Update" }));
    await user.click(await screen.findByRole("button", { name: "View report" }));

    // Assert
    const report = await screen.findByRole("dialog", { name: "Update report" });
    expect(within(report).getByText("3 elements converted; 0 errors, 1 warning, 0 notes.")).toBeTruthy();
    await user.click(within(report).getByRole("button", { name: "Warnings (1)" }));
    const warnings = within(report).getByRole("list", { name: "Warnings" });
    expect(within(warnings).getByLabelText("File core/feats.xml")).toBeTruthy();
    expect(warnings.textContent).toContain("(Feat ID_MOCK_CORE_FEAT, line 12 · dangling_reference)");
  });
});
