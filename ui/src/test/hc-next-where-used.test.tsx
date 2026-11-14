import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// H0eFo (spec 12.3): WHERE USED lists the pack's unsaved new drafts too (the backend's `draftId` rows), and one opens
// as its draft. The backend also counts a spell's derived tags and a class's `$(spellcasting:...)` terms (Rust tests
// in `src-tauri/tests/homebrew_expert.rs`); the mock's sample content holds no tag choices.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

async function saved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<string> {
  const v = await api.homebrewNewDraft({ packId, kind });
  const u = await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } });
  return (await api.homebrewSaveDraft({ draftId: u.draftId })).elementIds[0]!;
}

async function unsaved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<string> {
  const v = await api.homebrewNewDraft({ packId, kind });
  return (await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } })).draftId;
}

describe("H0eFo where used lists unsaved drafts (mock)", () => {
  it("an edited new draft that names the element is a row with its draft id; saved, it is a plain row", async () => {
    // Arrange
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const tongue = await saved(api, pack.id, "language", { name: "Gloomspeak" });

    // Act
    const draftId = await unsaved(api, pack.id, "feat", { name: "Gloom Adept", requirements: tongue });
    const before = await api.homebrewWhereUsed({ elementId: tongue });
    await api.homebrewSaveDraft({ draftId });
    const after = await api.homebrewWhereUsed({ elementId: tongue });

    // Assert
    expect(before).toEqual([expect.objectContaining({ name: "Gloom Adept", type: "Feat", packId: pack.id, relation: "requires", draftId })]);
    expect(after).toHaveLength(1);
    expect(after[0]!.draftId).toBeUndefined();
  });

  it("the row says it is not saved yet and opens the draft", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const tongue = await saved(api, pack.id, "language", { name: "Gloomspeak" });
    await unsaved(api, pack.id, "feat", { name: "Gloom Adept", requirements: tongue });
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: /^Languages \(1\)/ }));
    await user.click(await screen.findByRole("button", { name: "Gloomspeak" }));
    const where = await screen.findByRole("region", { name: "Where used" });
    expect(await within(where).findByText("Needs it · Feat · Not saved yet")).toBeTruthy();
    await user.click(within(where).getByRole("button", { name: "Gloom Adept" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^name$/i }) as HTMLInputElement).value).toBe("Gloom Adept"));
  });
});
