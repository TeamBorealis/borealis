import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { MOCK_KIND_DESCRIPTORS } from "../mock/homebrew-kinds";

// Hn_IU: the species and race Traits fields pick existing Racial Traits; the backend offers every loaded one, so the
// web mock's sample content has some.
describe("the web mock's Traits picker", () => {
  it.each(["species", "race"])("offers loaded Racial Traits to a %s", async (kind) => {
    const target = MOCK_KIND_DESCRIPTORS.find((k) => k.id === kind)!.fields.find((f) => f.key === "traits")!.picker!;
    const api = new MockApi({ creatorEnabled: true });
    const page = await api.homebrewReferenceOptions({ target, query: "", filters: {}, page: { offset: 0, limit: 50 } });
    expect(page.items.map((r) => r.name)).toEqual(expect.arrayContaining(["Darkvision", "Fey Ancestry", "Keen Senses", "Trance"]));
    expect(page.items.every((r) => r.type === "racial-trait")).toBe(true);
    const found = await api.homebrewReferenceOptions({ target, query: "dark", filters: {}, page: { offset: 0, limit: 50 } });
    expect(found.items.map((r) => r.name)).toEqual(["Darkvision"]);
  });
});
