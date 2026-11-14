import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type { HomebrewForm } from "../types";
import { formProblems } from "../mock/homebrew-check";
import { MOCK_BLOCK_CATALOG } from "../mock/homebrew-catalog";
import { MOCK_KIND_DESCRIPTORS } from "../mock/homebrew-kinds";
import { MockApi } from "../mock";

// IDprw: the web mock checks a class's skill choice as the backend does. The Rust test
// `crates/borealis-authoring/tests/mock_class_skills.rs` writes class forms and the problems the backend's check finds
// at their Skills field; the mock's check reads the same forms here.
interface Fixture { cases: { case: string; form: HomebrewForm; problems: { path: string; message: string }[] }[] }

const fixture = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-class-skills.json"), "utf8"),
) as Fixture;
const classKind = MOCK_KIND_DESCRIPTORS.find((k) => k.id === "class");

describe("the mock's class skill choice check", () => {
  it.each(fixture.cases.map((c) => [c.case, c] as const))("%s: as the backend finds it", (_, c) => {
    const found = formProblems(c.form, MOCK_BLOCK_CATALOG, null, classKind)
      .filter((p) => p.path === "fields.skills")
      .map((p) => ({ path: p.path, message: p.message }));
    expect(found).toEqual(c.problems);
  });

  it("refuses to save a new class until its skills are picked", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const { id: packId } = await api.homebrewCreatePack({ request: { name: "Gloom", code: "GL", edition: "dnd2024" } });
    const draft = await api.homebrewNewDraft({ packId, kind: "class" });
    const named = await api.homebrewUpdateDraft({ draftId: draft.draftId, form: { ...draft.form, fields: { ...draft.form.fields, name: "Gloom Knight" } } });
    expect(named.problems.filter((p) => p.path === "fields.skills").map((p) => p.message)).toEqual(["Pick the skills the character chooses from."]);
    await expect(api.homebrewSaveDraft({ draftId: draft.draftId })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});
