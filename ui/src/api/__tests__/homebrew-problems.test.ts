import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { formProblems, isDice, withArticle } from "../mock/homebrew-check";
import { MOCK_BLOCK_CATALOG } from "../mock/homebrew-catalog";
import type { HomebrewBlock, HomebrewForm, HomebrewKindDescriptor } from "../types";

// The mock's live problems say what the backend's `borealis_authoring::check` says (WB-07), word for word, for the codes
// the mock reproduces: required form and block fields, ranges and dice. The engine checks are the backend's only.

const PACK = { name: "Gloom", code: "GL", edition: "dnd2024" as const };

const form = (fields: Record<string, unknown>, blocks: HomebrewBlock[] = []): HomebrewForm =>
  ({ kind: "other", fields, blocks, children: [], preserved: [] });

const block = (kind: string, fields: Record<string, unknown>): HomebrewBlock => ({ key: `b-${kind}`, kind, sentence: "", fields });

describe("homebrew creator problems on the mock (WB-07)", () => {
  it("names an empty name and empty required fields in plain words", () => {
    const deity: HomebrewKindDescriptor = {
      id: "deity", label: "Deity", pluralLabel: "Deities", description: "", group: "small", elementTypes: ["Deity"], parentKinds: [],
      editions: ["dnd2014", "dnd2024"], tabs: [],
      fields: [
        { key: "name", label: "Name", control: "text", tab: "general", bindings: [], required: true },
        { key: "alignment", label: "Alignment", control: "text", tab: "general", bindings: [], required: true },
        { key: "domains", label: "Domains", control: "chips", tab: "general", bindings: [], required: true },
      ],
    };
    const got = formProblems({ ...form({ name: " ", domains: [] }), kind: "deity" }, MOCK_BLOCK_CATALOG, null, deity);
    expect(got.map((p) => [p.path, p.message])).toEqual([
      ["fields.name", "Give it a name."],
      ["fields.alignment", "A deity needs an alignment."],
      ["fields.domains", "A deity needs domains."],
    ]);
  });

  it("tells a block's missing value, a number out of range and dice that aren't dice", () => {
    const got = formProblems(form({ name: "Howl" }, [
      block("grant", { level: 25 }),
      block("healing", { dice: "lots" }),
    ]), MOCK_BLOCK_CATALOG, null);
    expect(got.map((p) => [p.severity, p.code, p.path, p.message])).toEqual([
      ["error", "block.required", "blocks.0.fields.id", "Gives: fill in what it gives."],
      ["error", "block.range", "blocks.0.fields.level", "Gives: from level must be a whole number from 1 to 20."],
      ["warning", "block.dice", "blocks.1.fields.dice", "Healing ability: write dice as dice, like 2d6 or 1d8+2."],
    ]);
  });

  it("reads dice and articles as the backend does", () => {
    expect(["2d6", "1d8+2", "d4", "1d6 + 1d4", "3"].every(isDice)).toBe(true);
    expect(["", "lots", "2d", "1d6+", "2x6"].some(isDice)).toBe(false);
    expect([withArticle("alignment"), withArticle("symbol"), withArticle("domains")]).toEqual(["an alignment", "a symbol", "domains"]);
  });

  it("has no engine checks to report: those need the engine", async () => {
    const api = new MockApi();
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const d = await api.homebrewNewDraft({ packId, kind: "spell" });
    expect(d.problems.map((p) => p.message)).toEqual(["Give it a name.", "A spell needs a school."]);
    expect(await api.homebrewCheckDraft({ draftId: d.draftId })).toEqual([]);
  });
});
