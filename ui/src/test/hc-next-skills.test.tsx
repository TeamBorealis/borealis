import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { MockApi } from "../api/mock/MockApi";
import { MOCK_BLOCK_CATALOG } from "../api/mock/homebrew-catalog";
import { FieldList } from "../pages/homebrew/blocks/BlockFieldControl";
import type { Fields } from "../pages/homebrew/blocks/values";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";
import { chooseOption, shownOption } from "./utils";

// HC16 (2), item IpY1E: the creator's "Bonus to skills" and "Advantage on a roll" blocks offer the installed homebrew
// skills after the eighteen (the backend's `Control::Skills`: the field's choices, then the names its "Skill Name" picker
// gives). The mock gives the same names as the backend: saved Skill elements that are character skills, the eighteen left
// out, a draft not saved yet left out (`homebrew_refs`, `form::skill_names`).

const PACK = { name: "Old World", code: "OW", edition: "dnd2014" as const };
const SKILL_NAME_PICKER = "Skill Name";
const spec = (kind: string) => MOCK_BLOCK_CATALOG.find((b) => b.kind === kind)!;
const field = (kind: string, key: string) => spec(kind).fields.find((f) => f.key === key)!;

async function skillDraft(api: MockApi, packId: string, name: string, ability = "Intelligence") {
  const v = await api.homebrewNewDraft({ packId, kind: "skill" });
  return api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, name, ability } } });
}

async function savedSkill(api: MockApi, packId: string, name: string, ability?: string) {
  const v = await skillDraft(api, packId, name, ability);
  await api.homebrewSaveDraft({ draftId: v.draftId });
}

async function skillWords(api: MockApi): Promise<string[]> {
  const page = await api.homebrewReferenceOptions({ target: { types: [SKILL_NAME_PICKER] }, query: "", filters: {}, page: { offset: 0, limit: 500 } });
  return page.items.map((r) => r.name);
}

async function withAlchemy(): Promise<MockApi> {
  const api = mockApi();
  const pack = await api.homebrewCreatePack({ request: PACK });
  await savedSkill(api, pack.id, "Alchemy");
  return api;
}

async function fieldsOf(kind: string, api: MockApi) {
  const onValue = vi.fn();
  await renderControl(
    <Stateful<Fields> initial={{}} onValue={onValue}>{(v, set) => <FieldList fields={spec(kind).fields} values={v} onValues={set} path="" problems={[]} />}</Stateful>,
    { api },
  );
  return onValue;
}

describe("the skill blocks offer the installed homebrew skills (HC16)", () => {
  it("their skill fields are the eighteen, then the names the Skill Name picker gives", async () => {
    for (const [kind, key, control] of [["skill_bonus", "skills", "chips"], ["roll_advantage", "skill", "combo"]] as const) {
      const f = field(kind, key);
      expect(f.control, `${kind}.${key}`).toBe(control);
      expect(f.choices?.length, `${kind}.${key}`).toBe(18);
      expect(f.allowCustom, `${kind}.${key}`).toBeUndefined();
      expect(f.picker?.types, `${kind}.${key}`).toEqual([SKILL_NAME_PICKER]);
    }
    const api = mockApi();
    const pack = await api.homebrewCreatePack({ request: PACK });
    expect(await skillWords(api)).toEqual([]);
    await savedSkill(api, pack.id, "Alchemy");
    // one of the eighteen, a name that is no skill's, a skill without an ability and a draft not saved: none offered
    await savedSkill(api, pack.id, "Arcana");
    await savedSkill(api, pack.id, "Luck", "Fortune");
    await skillDraft(api, pack.id, "Herbalism");
    await savedSkill(api, pack.id, "Sea  Lore", "wis");
    expect(await skillWords(api)).toEqual(["Alchemy", "Sea Lore"]);
  });

  it("Bonus to skills shows a chip per installed skill after the eighteen; pressing one writes its name", async () => {
    const user = userEvent.setup();
    const onValue = await fieldsOf("skill_bonus", await withAlchemy());
    const group = screen.getByRole("group", { name: "Skills" });
    await within(group).findByRole("button", { name: "Alchemy" });
    const chips = within(group).getAllByRole("button").map((b) => b.textContent);
    expect(chips).toEqual([...field("skill_bonus", "skills").choices!.map((c) => c.label), "Alchemy"]);
    await user.click(within(group).getByRole("button", { name: "Arcana" }));
    await user.click(within(group).getByRole("button", { name: "Alchemy" }));
    expect(onValue.mock.lastCall![0]).toEqual({ skills: ["Arcana", "Alchemy"] });
    expect(within(group).getByRole("button", { name: "Alchemy" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("Advantage on a roll's Skill lists the installed skills after the eighteen; picking one writes its name", async () => {
    const user = userEvent.setup();
    const onValue = await fieldsOf("roll_advantage", await withAlchemy());
    const box = document.querySelector('[data-field="skill"]') as HTMLElement;
    const select = within(box).getByRole("combobox", { name: "Skill" });
    await within(select).findByRole("option", { name: "Alchemy" });
    const options = within(select).getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Not set", ...field("roll_advantage", "skill").choices!.map((c) => c.label), "Alchemy"]);
    await chooseOption(user, "Skill", "Alchemy", box);
    expect(onValue.mock.lastCall![0]).toEqual({ skill: "Alchemy" });
    expect(shownOption("Skill", box)).toBe("Alchemy");
  });
});
