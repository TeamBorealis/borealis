import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, ClassSpellcasting, SpellEntry } from "../api/contract";
import { renderApp, seededId } from "./utils";

/**
 * A feat's spellcasting block (Magic Initiate) sits next to the class's on the Magic page. Every block has an id of its
 * own (`caster-...` for a block of no class), so the feat's tab shows the feat's spells and a prepare tick in its block
 * addresses the feat's block, never the class sharing the main class's id.
 */
const FEAT_ID = "caster-magic-initiate-wizard";
const FEAT = "Magic Initiate (Wizard)";

const spell = (id: string, name: string, level: number, alwaysPrepared: boolean): SpellEntry => ({
  id, name, level, school: "Evocation", prepared: alwaysPrepared, alwaysPrepared, origin: "class", slotId: null,
  canReplace: false, concentration: false, ritual: false, casting: null, sourceName: "", ready: true,
});

const withFeatBlock = (s: CharacterSnapshot): CharacterSnapshot => {
  const wizard = s.spellcasting.classes[0]!;
  const feat: ClassSpellcasting = {
    ...wizard, classSlotId: FEAT_ID, className: FEAT, model: "known", cantripsMax: 2, cantripsKnown: 2, preparedMax: 0,
    preparedCount: 0, maxSpellLevel: 0, pact: null,
    spells: [spell("ID_T_GLIMMER", "Glimmer Spark", 0, true), spell("ID_T_FROST", "Frost Whisper", 0, true),
      spell("ID_T_VEIL", "Hollow Veil", 1, false)],
  };
  return { ...s, spellcasting: { ...s.spellcasting, classes: [wizard, feat] } };
};

class FeatBlockApi extends MockApi {
  readonly prepared: Array<{ classSlotId: string; spellId: string }> = [];

  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withFeatBlock(await super.loadCharacter(args));
  }

  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withFeatBlock(await super.getSnapshot(args));
  }

  override async setSpellPrepared(a: { characterId: string; classSlotId: string; spellId: string; prepared: boolean }) {
    this.prepared.push({ classSlotId: a.classSlotId, spellId: a.spellId });
    return this.getSnapshot({ characterId: a.characterId });
  }
}

const block = (name: string) => screen.findByRole("region", { name: `${name} spellcasting` });

describe("Magic page: a feat's spellcasting block", () => {
  it("shows the feat's own spells on its tab, not the class's", async () => {
    const api = new FeatBlockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const user = userEvent.setup();
    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Spellcasting", "WIZARD", FEAT.toUpperCase()]);

    await user.click(screen.getByRole("tab", { name: FEAT.toUpperCase() }));
    const feat = within(await block(FEAT));
    const names = within(feat.getByRole("table", { name: `${FEAT} spells` })).getAllByRole("button").map((b) => b.textContent);
    expect(names).toEqual(["Glimmer Spark", "Frost Whisper", "Hollow Veil"]);
    expect(screen.queryByRole("region", { name: "Wizard spellcasting" })).toBeNull();
    expect(screen.getByRole("tab", { name: FEAT.toUpperCase() }).getAttribute("aria-selected")).toBe("true");

    await user.click(screen.getByRole("tab", { name: "WIZARD" }));
    expect(await block("Wizard")).toBeTruthy();
    expect(screen.queryByRole("region", { name: `${FEAT} spellcasting` })).toBeNull();
  });

  it("a prepare tick in the feat's block names the feat's block", async () => {
    const api = new FeatBlockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/spells", { api, openCharacterId: id });
    const user = userEvent.setup();
    const feat = within(await block(FEAT));
    await user.click(feat.getByRole("checkbox", { name: "Prepare Hollow Veil" }));
    expect(api.prepared).toEqual([{ classSlotId: FEAT_ID, spellId: "ID_T_VEIL" }]);
  });
});
