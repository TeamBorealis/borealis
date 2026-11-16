import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, SpellEntry } from "../api/contract";
import { classesScope, EMPTY_FILTER } from "../pages/spells/SpellFilter";
import { classRowCount, classSpellRows, fixedReason } from "../pages/spells/classSpellRows";
import { renderApp, seededId } from "./utils";

/**
 * GitHub #51 piece 2: a prepared caster's class spell list shows one row per name, with the other copies in the row's
 * versions menu and the copy always used (piece 1) as the row's default.
 */

const spell = (id: string, name: string, sourceName: string, extra: Partial<SpellEntry> = {}): SpellEntry => ({
  id, name, level: 3, school: "Evocation", prepared: false, alwaysPrepared: false, origin: "class", slotId: null,
  canReplace: false, concentration: false, ritual: false, casting: null, sourceName, ready: false, rulesets: ["dnd2024"], ...extra,
});
const FB_SRD = spell("fb-srd", "Fireball", "SRD 5.2.1", { bundled: true });
const FB_HOME = spell("fb-home", "Fireball", "Table Homebrew");
const FB_OLD = spell("fb-old", "Fireball", "SRD 5.1", { bundled: true, rulesets: ["dnd2014"] });
const SHIELD = spell("shield-srd", "Shield", "SRD 5.2.1", { level: 1, school: "Abjuration", bundled: true });
const KEY_2024 = "spell:dnd2024:fireball";

const ids = (rows: { spell: SpellEntry }[]) => rows.map((r) => r.spell.id);

describe("the rows of a class list", () => {
  const list = [SHIELD, FB_SRD, FB_HOME, FB_OLD];

  it("puts the copies of one name in one row that shows the default copy", () => {
    const rows = classSpellRows(list, "dnd2024", {}, {});
    expect(ids(rows)).toEqual(["shield-srd", "fb-home"]);
    expect(rows[1]!.copies.map((c) => c.id)).toEqual(["fb-srd", "fb-home", "fb-old"]);
    expect(classRowCount(list)).toBe(2);
  });

  it("shows the menu pick, else the copy always used for the character's edition", () => {
    expect(ids(classSpellRows(list, "dnd2024", {}, { [KEY_2024]: "fb-srd" }))).toEqual(["shield-srd", "fb-srd"]);
    expect(ids(classSpellRows(list, "dnd2014", {}, { [KEY_2024]: "fb-srd" }))).toEqual(["shield-srd", "fb-old"]);
    const key = classSpellRows(list, "dnd2024", {}, {})[1]!.key;
    expect(ids(classSpellRows(list, "dnd2024", { [key]: "fb-old" }, { [KEY_2024]: "fb-srd" }))).toEqual(["shield-srd", "fb-old"]);
  });

  it("a prepared copy keeps its row and the other copies join its menu; each prepared copy has a row", () => {
    const one = classSpellRows([SHIELD, FB_SRD, { ...FB_HOME, prepared: true }, FB_OLD], "dnd2024", {}, { [KEY_2024]: "fb-srd" });
    expect(ids(one)).toEqual(["shield-srd", "fb-home"]);
    expect(one[1]!.held).toBe(true);
    expect(one[1]!.copies.map((c) => c.id)).toEqual(["fb-home", "fb-srd", "fb-old"]);
    const two = [SHIELD, { ...FB_SRD, prepared: true }, { ...FB_HOME, prepared: true }, FB_OLD];
    expect(ids(classSpellRows(two, "dnd2024", {}, {}))).toEqual(["shield-srd", "fb-srd", "fb-home"]);
    expect(classRowCount(two)).toBe(3);
  });

  it("says why a copy can't be switched: slot, grant, or added in the builder", () => {
    expect(fixedReason({ ...FB_HOME, slotId: "s1" })).toBe("Switch it in its spell slot");
    expect(fixedReason({ ...FB_HOME, grantedFrom: "Feature" })).toBe("Granted as the shown copy");
    expect(fixedReason({ ...FB_HOME, added: true })).toBe("Added as the shown copy");
  });

  it("the filter bar counts rows", () => {
    const cls = { spells: list } as Parameters<typeof classesScope>[0][number];
    expect(classesScope([cls], EMPTY_FILTER)).toMatchObject({ shown: 2, total: 2 });
    expect(classesScope([cls], { ...EMPTY_FILTER, source: "SRD 5.1" })).toMatchObject({ shown: 1, total: 2 });
  });
});

/** A mock whose first class is a 2024 prepared caster listing three copies of Fireball. */
class CopiesApi extends MockApi {
  readonly prepared = new Set<string>();
  readonly calls: Array<[string, boolean]> = [];

  private withCopies(s: CharacterSnapshot): CharacterSnapshot {
    const [first, ...rest] = s.spellcasting.classes;
    const spells = [SHIELD, FB_SRD, FB_HOME, FB_OLD].map((x) => ({ ...x, prepared: this.prepared.has(x.id) }));
    const cls = { ...first!, model: "prepared" as const, preparedMax: 4, spells };
    return { ...s, ruleset: "dnd2024", spellcasting: { ...s.spellcasting, classes: [cls, ...rest] } };
  }

  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return this.withCopies(await super.loadCharacter(args));
  }

  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return this.withCopies(await super.getSnapshot(args));
  }

  /** A copy that can't be prepared (the second call of a switch fails). */
  refuse: string | null = null;

  override async setSpellPrepared(a: { characterId: string; classSlotId: string; spellId: string; prepared: boolean }) {
    this.calls.push([a.spellId, a.prepared]);
    if (a.prepared && a.spellId === this.refuse) throw new Error("The prepared limit is reached.");
    if (a.prepared) this.prepared.add(a.spellId);
    else this.prepared.delete(a.spellId);
    return this.getSnapshot({ characterId: a.characterId });
  }
}

async function openMagic(api: CopiesApi) {
  const id = await seededId(api, "Seraphine");
  renderApp("/spells", { api, openCharacterId: id });
  const table = await screen.findByRole("table", { name: /spells$/ });
  return table;
}

const fireballRow = (table: HTMLElement) => within(table).getByRole("button", { name: "Fireball" }).closest("tr")!;

describe("the Magic page's class list", () => {
  it("shows one Fireball row, and 'Always use this copy' makes the SRD copy its default", async () => {
    const api = new CopiesApi();
    const table = await openMagic(api);
    expect(within(table).getAllByRole("button", { name: "Fireball" })).toHaveLength(1);
    expect(fireballRow(table).querySelector(".sp-src")!.textContent).toBe("Table Homebrew");

    const user = userEvent.setup();
    await user.click(within(table).getByRole("button", { name: "3 versions of Fireball" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "SRD 5.2.1" }));
    expect(fireballRow(table).querySelector(".sp-src")!.textContent).toBe("SRD 5.2.1");
    await user.click(within(table).getByRole("button", { name: "3 versions of Fireball" }));
    await user.click(screen.getByRole("checkbox", { name: "Always use the SRD 5.2.1 copy" }));
    await waitFor(async () => expect((await api.getSettings()).preferredCopies).toEqual({ [KEY_2024]: "fb-srd" }));
    expect(api.calls).toEqual([]);
  });

  it("starts on the copy always used, and prepares the copy the row shows", async () => {
    const api = new CopiesApi();
    api.presetSettings({ preferredCopies: { [KEY_2024]: "fb-srd" } });
    const table = await openMagic(api);
    await waitFor(() => expect(fireballRow(table).querySelector(".sp-src")!.textContent).toBe("SRD 5.2.1"));
    await userEvent.setup().click(within(fireballRow(table)).getByRole("checkbox", { name: "Prepare Fireball" }));
    await waitFor(() => expect(api.calls).toEqual([["fb-srd", true]]));
  });

  it("picking another copy on a prepared row moves the preparation to it", async () => {
    const api = new CopiesApi();
    api.prepared.add("fb-home");
    const table = await openMagic(api);
    const user = userEvent.setup();
    await user.click(within(table).getByRole("button", { name: "3 versions of Fireball" }));
    const menu = await screen.findByRole("listbox");
    expect(within(menu).getByRole("option", { selected: true }).getAttribute("aria-label")).toBe("Table Homebrew, chosen");
    await user.click(within(menu).getByRole("option", { name: "SRD 5.2.1" }));
    await waitFor(() => expect(api.calls).toEqual([["fb-home", false], ["fb-srd", true]]));
    await waitFor(() => expect(fireballRow(table).querySelector(".sp-src")!.textContent).toBe("SRD 5.2.1"));
    expect(within(table).getAllByRole("button", { name: "Fireball" })).toHaveLength(1);
  });

  it("a switch whose second step fails keeps the copy the character had prepared", async () => {
    const api = new CopiesApi();
    api.prepared.add("fb-home");
    api.refuse = "fb-srd";
    const table = await openMagic(api);
    const user = userEvent.setup();
    await user.click(within(table).getByRole("button", { name: "3 versions of Fireball" }));
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "SRD 5.2.1" }));
    await waitFor(() => expect(api.calls).toEqual([["fb-home", false], ["fb-srd", true], ["fb-home", true]]));
    expect([...api.prepared]).toEqual(["fb-home"]);
  });
});
