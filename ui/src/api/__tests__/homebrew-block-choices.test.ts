import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { MockApi } from "../mock";
import { formProblems } from "../mock/homebrew-check";
import { MOCK_BLOCK_CATALOG } from "../mock/homebrew-catalog";
import type { HomebrewBlock, HomebrewForm } from "../types";

// Item I0Yxk: the web mock refuses a block value outside its field's closed choices, as the backend's form check does
// (`borealis_authoring::check::fields`: `value_findings` and `Finding::BlockChoice`, code `block.choice`, "<block>: pick
// <field> from the list."). A closed combo or chips value that is not a choice; a skill field's name that is neither one
// of the eighteen nor a loaded homebrew skill (`blocks::fit::is_skill`, HC16); a variant of an unknown kind; parts of
// groups, rows and maps; a toggle's effects. Open fields (`allowCustom`) take any text, and the readers that show
// fields verbatim take a single choice where the block writes a list (`blocks::fit::misfits_with`).

const PACK = { name: "Old World", code: "OW", edition: "dnd2014" as const };

const form = (blocks: HomebrewBlock[]): HomebrewForm => ({ kind: "other", fields: { name: "Howl" }, blocks, children: [], preserved: [] });
const block = (kind: string, fields: Record<string, unknown>): HomebrewBlock => ({ key: `b-${kind}`, kind, sentence: "", fields });

/** The closed-choice and shape problems of one block, as `[code, path, message]`. */
const problemsOf = (b: HomebrewBlock, skills: readonly string[] = []) =>
  formProblems(form([b]), MOCK_BLOCK_CATALOG, null, undefined, skills)
    .filter((p) => p.code === "block.choice" || p.code === "block.shape")
    .map((p) => [p.code, p.path, p.message]);

const choice = (path: string, message: string) => ["block.choice", `blocks.0.fields.${path}`, message];

describe("the mock refuses block values outside their closed choices (I0Yxk)", () => {
  it("a closed combo or chips value that is not one of the choices", () => {
    expect(problemsOf(block("grant_spell", { spell: { id: "ID_X", name: "X", type: "Spell" }, mode: "sometimes" })))
      .toEqual([choice("mode", "Gives a spell: pick how from the list.")]);
    expect(problemsOf(block("pact_weapon", { damage_types: ["fire", "plasma"] })))
      .toEqual([choice("damage_types", "Pact weapon: pick damage types from the list.")]);
    expect(problemsOf(block("healing", { name: "Mend", dice: "1d8", action: "minute" })))
      .toEqual([choice("action", "Healing ability: pick takes from the list.")]);
  });

  it("a combo or chips value of another shape is a choice the list doesn't have, as the backend says", () => {
    expect(problemsOf(block("sense", { sense: 60 }))).toEqual([choice("sense", "Special sense: pick sense from the list.")]);
    expect(problemsOf(block("resource", { id: "Embers", max: { type: "fixed", value: 2 }, recharge: "long_rest" })))
      .toEqual([choice("recharge", "Limited uses / pool: pick regained on from the list.")]);
  });

  it("takes the choices, any text in an open field, and a single choice where a verbatim reader writes a list", () => {
    expect(problemsOf(block("grant_spell", { spell: { id: "ID_X", name: "X", type: "Spell" }, mode: "known" }))).toEqual([]);
    expect(problemsOf(block("sense", { sense: "echolocation", range: 30 }))).toEqual([]);
    expect(problemsOf(block("pact_weapon", { damage_types: "fire" }))).toEqual([]);
    expect(problemsOf(block("pact_weapon", { damage_types: "plasma" })))
      .toEqual([choice("damage_types", "Pact weapon: pick damage types from the list.")]);
  });

  it("a variant of a kind the field doesn't have", () => {
    expect(problemsOf(block("resource", { id: "Embers", max: { type: "dice", value: "1d4" } })))
      .toEqual([choice("max", "Limited uses / pool: pick maximum from the list.")]);
    expect(problemsOf(block("resource", { id: "Embers", max: { type: "fixed", value: 2 } }))).toEqual([]);
  });

  it("parts of a group and of a map's values, by the part's label", () => {
    expect(problemsOf(block("grant_spell", { spell: { id: "ID_X", name: "X", type: "Spell" }, free_cast: { uses: 1, recharge: "weekly" } })))
      .toEqual([choice("free_cast.recharge", "Gives a spell: pick regained on from the list.")]);
    // a map's key outside its closed choices is a map the backend can't read
    expect(problemsOf(block("resource", { id: "Embers", max: { type: "fixed", value: 2 }, regain: { long_rest: 1 } }))).toEqual([]);
    expect(problemsOf(block("resource", { id: "Embers", max: { type: "fixed", value: 2 }, regain: { dawn: 1 } })))
      .toEqual([["block.shape", "blocks.0.fields.regain", "This value can't be read. Undo the last change or enter it again."]]);
  });

  it("a toggle's effects, at the effect's path and by its block's label", () => {
    const effect = { key: "e0", kind: "roll_advantage", sentence: "", fields: { roll: "ability_check", skill: "Alchemy" } };
    expect(problemsOf(block("toggle", { id: "rage", name: "Rage", effects: [effect] })))
      .toEqual([choice("effects.0.fields.skill", "Advantage on a roll: pick skill from the list.")]);
    expect(problemsOf(block("toggle", { id: "rage", name: "Rage", effects: [effect] }), ["alchemy"])).toEqual([]);
  });

  it("a skill field takes the eighteen and the loaded homebrew skills by name, any case and spacing", () => {
    expect(problemsOf(block("skill_bonus", { skills: ["Arcana", "Alchemy"] })))
      .toEqual([choice("skills", "Bonus to skills: pick skills from the list.")]);
    expect(problemsOf(block("roll_advantage", { roll: "ability_check", skill: "Alchemy" })))
      .toEqual([choice("skill", "Advantage on a roll: pick skill from the list.")]);
    expect(problemsOf(block("skill_bonus", { skills: ["Arcana", "Alchemy"] }), ["alchemy"])).toEqual([]);
    expect(problemsOf(block("roll_advantage", { roll: "ability_check", skill: "alchemy" }), ["alchemy"])).toEqual([]);
    expect(problemsOf(block("roll_advantage", { roll: "ability_check", skill: "Sea  Lore" }), ["sea lore"])).toEqual([]);
    // the eighteen by their own spelling only: "arcana" is the roll's choice, not the bonus's
    expect(problemsOf(block("skill_bonus", { skills: ["arcana"] }), ["alchemy"]))
      .toEqual([choice("skills", "Bonus to skills: pick skills from the list.")]);
  });

  it("every block of the backend's sample form is within its choices", () => {
    const sample = (JSON.parse(fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-form.json"), "utf8")) as { form: HomebrewForm }).form;
    expect(formProblems(sample, MOCK_BLOCK_CATALOG, null).filter((p) => p.code === "block.choice")).toEqual([]);
  });

  it("a draft's skill block names a skill a saved Skill element adds, not one of a draft not saved", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const feat = await api.homebrewNewDraft({ packId, kind: "feat" });
    const withBlock = { ...feat.form, fields: { ...feat.form.fields, name: "Brewer" }, blocks: [block("skill_bonus", { skills: ["Alchemy"] })] };
    const choiceProblems = async () => (await api.homebrewUpdateDraft({ draftId: feat.draftId, form: withBlock })).problems
      .filter((p) => p.code === "block.choice").map((p) => [p.path, p.message]);
    expect(await choiceProblems()).toEqual([["blocks.0.fields.skills", "Bonus to skills: pick skills from the list."]]);
    const skill = await api.homebrewNewDraft({ packId, kind: "skill" });
    const named = await api.homebrewUpdateDraft({ draftId: skill.draftId, form: { ...skill.form, fields: { ...skill.form.fields, name: "Alchemy", ability: "Intelligence" } } });
    expect(await choiceProblems()).toEqual([["blocks.0.fields.skills", "Bonus to skills: pick skills from the list."]]);
    await api.homebrewSaveDraft({ draftId: named.draftId });
    expect(await choiceProblems()).toEqual([]);
  });
});
