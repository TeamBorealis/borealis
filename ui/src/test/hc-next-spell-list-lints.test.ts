import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";

// Spell list lints (board Hlwvg): a list no loaded class declares is a warning naming the near spelling, and a spell
// on no list is a hint, as the backend's `check::lints` writes them (`lint.unknown_spell_list`, `lint.no_spell_list`).

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

async function spellWith(lists: string[]) {
  const api = new MockApi({ creatorEnabled: true });
  const pack = await api.homebrewCreatePack({ request: PACK });
  const draft = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  const view = await api.homebrewUpdateDraft({ draftId: draft.draftId, form: { ...draft.form, fields: { ...draft.form.fields, name: "Glimmer", school: "Illusion", spellLists: lists } } });
  return view.problems.filter((p) => p.path === "fields.spellLists").map((p) => [p.code, p.severity, p.message]);
}

describe("spell list lints (mock)", () => {
  it("a declared list is clean; a misspelled or unknown one is a warning", async () => {
    expect(await spellWith(["Wizard"])).toEqual([]);
    expect(await spellWith(["Wizard", "Wizzard"])).toEqual([
      ["lint.unknown_spell_list", "warning", "No loaded class or choice uses the spell list \"Wizzard\". Check the spelling: there is a list \"Wizard\"."],
    ]);
    expect(await spellWith(["wizard"])).toEqual([
      ["lint.unknown_spell_list", "warning", "No loaded class or choice uses the spell list \"wizard\". Lists match by exact spelling: there is a list \"Wizard\"."],
    ]);
    expect(await spellWith(["Artificer"])).toEqual([
      ["lint.unknown_spell_list", "warning", "No loaded class or choice uses the spell list \"Artificer\". Check the spelling, or make the class that has it."],
    ]);
  });

  it("a spell on no list is a hint", async () => {
    expect(await spellWith([])).toEqual([
      ["lint.no_spell_list", "info", "This spell is on no spell list, so no class offers it. It can still be given by a feature."],
    ]);
  });
});
