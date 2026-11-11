import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { attackText, crProficiency, diceAverage, entryTitle, standardXp, usualPassive } from "../pages/homebrew/creature/creature";
import { creatableKinds } from "../pages/homebrew/NewElementFlyout";
import { renderApp } from "./utils";

// Homebrew WB-20 (spec 4.11, 13.4, 13.6): the creature form on the mock, which serves the real descriptor (generated
// from `borealis-authoring`): the stat block in its order over GENERAL, STATISTICS, ACTIONS and LEGENDARY, its entries
// in the Attacks list editor with their dialog, the validator's sentences at the fields and the live stat block preview.

const editor = () => screen.getByRole("region", { name: "Element editor" });
const sections = () => [...editor().querySelectorAll(".hb-section-title")].map((h) => h.textContent);
const tabs = () => within(editor()).getAllByRole("tab").map((t) => t.textContent);

describe("WB-20 creature form (mock)", () => {
  it("the mock's creature form is the backend's: a main kind writing Monster, with the stat block's controls", () => {
    const k = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "creature")!;
    expect(k).toMatchObject({ group: "main", elementTypes: ["Monster"], editions: ["dnd2014", "dnd2024"] });
    const f = new Map(k.fields.map((x) => [x.key, x]));
    expect(f.get("armorClass")).toMatchObject({ control: "armorClass", tab: "general", section: "Armor class and hit points" });
    expect(f.get("challenge")?.choices?.length).toBe(34);
    expect(f.get("initiative")).toMatchObject({ control: "initiative", editions: ["dnd2024"] });
    expect(f.get("saves")).toMatchObject({ control: "bonuses", tab: "statistics" });
    expect(f.get("actions")).toMatchObject({ control: "entries", tab: "actions", default: [] });
    expect(f.get("lairActions")).toMatchObject({ control: "lair", tab: "legendary" });
    for (const gone of ["requirements", "allowDuplicate", "sheet"]) expect(f.has(gone)).toBe(false);
    expect(creatableKinds(MOCK_KIND_DESCRIPTORS).map((d) => d.id)).toContain("creature");
  });

  it("works out averages, XP, proficiency, passive Perception and an attack's words", () => {
    expect(diceAverage("2d8+2")).toBe(11);
    expect(diceAverage("2d")).toBeNull();
    expect(standardXp("1/4")).toBe(50);
    expect(crProficiency("5")).toBe(3);
    expect(crProficiency("1/2")).toBe(2);
    expect(usualPassive({ wis: 14 }, {})).toBe(12);
    expect(usualPassive({ wis: 14 }, { perception: 5 })).toBe(15);
    const bite = { kind: "melee" as const, to_hit: 4, reach: 5, damage: [{ average: 5, dice: "1d6+2", type: "piercing" }] };
    expect(attackText(bite, "dnd2024")).toBe("Melee Attack Roll: +4, reach 5 ft. Hit: 5 (1d6 + 2) Piercing damage.");
    expect(attackText({ ...bite, kind: "melee_weapon" }, "dnd2014")).toBe("Melee Weapon Attack: +4 to hit, reach 5 ft., one target. Hit: 5 (1d6 + 2) piercing damage.");
    expect(entryTitle({ name: "Fire Breath", text: "x", usage: { recharge: { min: 5, max: 6 } } })).toBe("Fire Breath (Recharge 5–6)");
  });

  it("a new creature: type, armor class, hit dice, challenge and an attack action, then saved", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Creature A stat block/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Cinder Boar");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Cinder Boar"));
    expect(tabs()).toEqual(["GENERAL", "STATISTICS", "ACTIONS", "LEGENDARY", "RULES", "DESCRIPTION", "DATA"]);
    expect(sections()).toEqual(["Size, type and alignment", "Armor class and hit points", "Speed and initiative", "Challenge", "Details"]);
    await waitFor(() => expect(screen.getAllByText("A creature needs a creature type.").length).toBeGreaterThan(0));

    await user.selectOptions(within(editor()).getByRole("combobox", { name: /Creature type/ }), "beast");
    await waitFor(() => expect(screen.getAllByText("Give the creature an Armor Class above 0, or words that say how it is worked out.").length).toBeGreaterThan(0));
    await user.click(within(editor()).getByRole("button", { name: "Add an armor class" }));
    await user.type(within(editor()).getByRole("textbox", { name: "Armor class 1: from" }), "natural armor");
    await user.type(within(editor()).getByRole("textbox", { name: "Hit points: hit dice" }), "2d8+2");
    await waitFor(() => expect(last().fields.hitPoints).toEqual({ average: 11, formula: "2d8+2", note: "" }));
    await user.selectOptions(within(editor()).getByRole("combobox", { name: "Challenge rating" }), "1/4");
    expect((within(editor()).getByRole("textbox", { name: "XP" }) as HTMLInputElement).value).toBe("50");
    await waitFor(() => expect(last().fields.challenge).toMatchObject({ cr: "1/4", xp: null }));

    await user.click(within(editor()).getByRole("tab", { name: "ACTIONS" }));
    await user.click(within(editor()).getByRole("button", { name: "Add action" }));
    const dialog = await screen.findByRole("dialog", { name: "New action" });
    await user.type(within(dialog).getByRole("textbox", { name: /^Name/ }), "Tusk");
    await user.click(within(dialog).getByRole("checkbox", { name: "Attack" }));
    await user.click(within(dialog).getByRole("button", { name: "Write the text from the attack" }));
    expect((within(dialog).getByRole("textbox", { name: /^Text/ }) as HTMLTextAreaElement).value).toBe("Melee Attack Roll: +3, reach 5 ft. Hit: 1 Slashing damage.");
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(last().fields.actions).toEqual([expect.objectContaining({ name: "Tusk", attack: expect.objectContaining({ kind: "melee", to_hit: 3 }) })]));
    expect(within(editor()).getByRole("listitem", { name: "Tusk" })).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    // the live preview is the compendium's stat block
    await waitFor(() => expect(screen.getAllByText("1/4 (50 XP)").length).toBeGreaterThan(0));
    expect(screen.getAllByText("11 (2d8+2)").length).toBeGreaterThan(0);

    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Cinder Boar" })).toBeTruthy();
  });
});
