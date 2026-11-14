import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { targetNoun } from "../pages/homebrew/pickers/refRows";

// HC1 (item HtCY8): a Skill made in the creator is a character skill. The skill pickers (a background's skill
// proficiencies, a class's skill choice) offer homebrew Skill elements with the SRD skills' proficiencies, and a Skill
// whose name the sheet can't use (an ability, a statistic, a colon) is refused at its name, on both backends
// (`check::fields`, `form.skill_name`).

const PACK = { name: "Old World", code: "OW", edition: "dnd2014" as const };
const picker = (kind: string, key: string) => MOCK_KIND_DESCRIPTORS.find((d) => d.id === kind)!.fields.find((f) => f.key === key)!.picker!;

async function skill(api: MockApi, packId: string, name: string) {
  const v = await api.homebrewNewDraft({ packId, kind: "skill" });
  return api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, name, ability: "Intelligence" } } });
}

describe("homebrew skills are character skills (mock)", () => {
  it("the background's and the class's skill pickers offer Skill elements too", async () => {
    expect(picker("background", "skills")).toMatchObject({ types: ["Proficiency", "Skill"], tags: ["Skill"] });
    expect(picker("class", "skills")).toMatchObject({ types: ["Proficiency", "Skill"] });
    // the picker still reads "Choose proficiencies", as before homebrew skills were offered
    expect(targetNoun(picker("background", "skills"))).toBe("proficiencies");
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const v = await skill(api, pack.id, "Alchemy");
    expect(v.problems.filter((p) => p.severity === "error")).toEqual([]);
    await api.homebrewSaveDraft({ draftId: v.draftId });
    const rows = await api.homebrewReferenceOptions({
      target: { types: ["Proficiency", "Skill"], tags: ["Skill"], multiple: true }, query: "alch", filters: {}, page: { offset: 0, limit: 50 },
    });
    expect(rows.items.map((r) => [r.name, r.type])).toEqual([["Alchemy", "Skill"]]);
  });

  it("a skill named like a statistic is refused at its name; another name is not", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    for (const bad of ["Strength", "ac", "Innate  Speed", "Lore: Tides"]) {
      const v = await skill(api, pack.id, bad);
      expect(v.problems.filter((p) => p.code === "form.skill_name").map((p) => [p.path, p.message]), bad).toEqual([[
        "fields.name", "A skill can't be named like an ability or another statistic (Strength, AC, Level), or hold a colon: rename it.",
      ]]);
    }
    for (const good of ["Linguistics", "Lore (Tides)", "Arcana"]) {
      const v = await skill(api, pack.id, good);
      expect(v.problems.filter((p) => p.code === "form.skill_name"), good).toEqual([]);
    }
  });
});
