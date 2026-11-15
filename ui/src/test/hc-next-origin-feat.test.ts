import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";

// The origin feat picker offers origin feats only (board Htzhw): the background's originFeat field picks feats tagged
// "Origin Feat" (as the 2024 SRD tags them), and the mock's sample origin feats carry that tag.

describe("origin feat picker (mock)", () => {
  it("offers the feats tagged Origin Feat", async () => {
    const field = MOCK_KIND_DESCRIPTORS.find((k) => k.id === "background")!.fields.find((f) => f.key === "originFeat")!;
    expect(field.picker).toEqual({ types: ["Feat"], tags: ["Origin Feat"] });
    const api = new MockApi();
    const page = await api.homebrewReferenceOptions({ target: field.picker!, query: "", filters: {}, page: { offset: 0, limit: 100 } });
    expect(page.items.map((r) => r.name).sort()).toEqual(["Alert", "Magic Initiate", "Savage Attacker", "Skilled"]);
  });
});
