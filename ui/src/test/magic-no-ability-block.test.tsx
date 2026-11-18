import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, ClassSpellcasting, SpellEntry } from "../api/contract";
import { renderApp, seededId } from "./utils";

/**
 * GitHub #68: a spellcasting block without a casting ability blanked the whole Magic page. The engine sends such a block
 * with an empty `ability` (1.4.0: an Aurora-format feat's spell picks, "Magic Initiate"; still today: an equipped item
 * that teaches a spell and names no ability). The band looked the ability up by key and threw, which unmounted the app.
 * Such a block now shows with blank Ability, Attack modifier and Save DC (no made-up +0 / 0), like the sheet's boxes,
 * and one block that cannot render no longer takes the rest of the page with it.
 */
const spell = (id: string, name: string, level: number): SpellEntry => ({
  id, name, level, school: "Evocation", prepared: true, alwaysPrepared: true, origin: "class", slotId: null,
  canReplace: false, concentration: false, ritual: false, casting: null, sourceName: "", ready: true,
});

const noAbilityBlock = (from: ClassSpellcasting, classSlotId: string, className: string): ClassSpellcasting => ({
  ...from, classSlotId, className, ability: "", saveDc: 0, attackBonus: 0, attackRoll: 0, model: "known", cantripsMax: 1,
  cantripsKnown: 1, preparedMax: 0, preparedCount: 0, maxSpellLevel: 0, pact: null,
  spells: [spell("ID_T_GLIMMER", "Glimmer Spark", 0)],
});

class ExtraBlockApi extends MockApi {
  constructor(private readonly extra: (wizard: ClassSpellcasting) => ClassSpellcasting) { super(); }

  private add(s: CharacterSnapshot): CharacterSnapshot {
    const wizard = s.spellcasting.classes[0]!;
    return { ...s, spellcasting: { ...s.spellcasting, classes: [wizard, this.extra(wizard)] } };
  }

  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return this.add(await super.loadCharacter(args));
  }

  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return this.add(await super.getSnapshot(args));
  }
}

const region = (name: string) => screen.findByRole("region", { name: `${name} spellcasting` });

/** The band's `[label, value]` stats of a block, as shown. */
const stats = (block: HTMLElement): Record<string, string> => Object.fromEntries(
  Array.from(block.querySelectorAll(".ap-band .ap-stats > span")).map((s) => {
    const value = s.querySelector("b")?.textContent ?? "";
    return [(s.textContent ?? "").slice(0, (s.textContent ?? "").length - value.length), value];
  }),
);

const SHAPES: Array<[string, string, string]> = [
  ["an Aurora-format feat's picks (1.4.0)", "caster-magic-initiate", "Magic Initiate"],
  ["an equipped item's spell", "caster-test-circlet", "Test Circlet"],
];

describe("Magic page: a spellcasting block without a casting ability (#68)", () => {
  for (const [what, id, name] of SHAPES) {
    it(`shows ${what} next to the class, with blank ability, attack and save DC`, async () => {
      const api = new ExtraBlockApi((w) => noAbilityBlock(w, id, name));
      const charId = await seededId(api, "Seraphine");
      renderApp("/spells", { api, openCharacterId: charId });
      const user = userEvent.setup();

      const wizard = await region("Wizard");
      const block = await region(name);
      expect(stats(wizard)["Ability:"]).toBe("INT");
      const shown = stats(block);
      expect(shown["Ability:"]).toBe("");
      expect(shown["Attack modifier:"]).toBe("");
      expect(shown["Save DC:"]).toBe("");
      expect(shown["Cantrips:"]).toBe("1/1");
      expect(within(block).getByRole("button", { name: "Glimmer Spark" })).toBeTruthy();

      // its own tab renders too, and leaving and coming back keeps the page
      await user.click(screen.getByRole("tab", { name: name.toUpperCase() }));
      expect(stats(await region(name))["Ability:"]).toBe("");
      await user.click(screen.getByRole("tab", { name: "Spellcasting" }));
      expect(await region("Wizard")).toBeTruthy();
      expect(await region(name)).toBeTruthy();
    });
  }

  it("one block that cannot render leaves the rest of the page in place", async () => {
    // a pact pool no array can hold: its slot tiles throw while rendering
    const broken = (w: ClassSpellcasting): ClassSpellcasting =>
      ({ ...noAbilityBlock(w, "caster-broken", "Broken Block"), pact: { level: 1, total: Number.POSITIVE_INFINITY } });
    const api = new ExtraBlockApi(broken);
    const charId = await seededId(api, "Seraphine");
    // React reports the caught render error on the console; the test only reads the page
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      renderApp("/spells", { api, openCharacterId: charId });
      expect(await region("Wizard")).toBeTruthy();
      expect(await screen.findByRole("table", { name: "Selection slots" })).toBeTruthy();
      expect((await screen.findByRole("alert")).textContent).toContain("Broken Block");
    } finally {
      quiet.mockRestore();
    }
  });
});
