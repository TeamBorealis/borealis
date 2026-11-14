import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { MOCK_KIND_DESCRIPTORS } from "../mock/homebrew-kinds";

// HmHDY: the web mock serves one generated file of descriptors (`homebrew-kinds.ts`, written by
// `src-tauri/tests/contract_homebrew.rs` from `homebrew_kinds`), every kind in the backend's order, items included.
describe("the web mock's creator kinds", () => {
  it("are the backend's descriptors, in the backend's order", async () => {
    const kinds = await new MockApi().homebrewKinds();
    expect(kinds).toEqual(MOCK_KIND_DESCRIPTORS);
    expect(kinds.map((k) => k.id)).toEqual(expect.arrayContaining(["item", "weapon", "armor", "magicItem", "itemPack", "equipmentPackage"]));
  });
});
