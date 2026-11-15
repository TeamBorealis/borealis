import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

// P3 Hy69k (HC11, CS1, HC8): a homebrew pack holding text copied from the user's imports is not redistributable, and
// its chip says why in its own words (no book is named), not the local-import wording.

const mock = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });

/** A homebrew pack with one spell copied from loaded content that is not SRD (the mock's stand-in for an import). */
async function packWithCopiedText(api: MockApi): Promise<void> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const rows = await api.homebrewReferenceOptions({ target: { types: ["Spell"] }, query: "", filters: { srdOnly: false }, page: { offset: 0, limit: 50 } });
  const loaded = rows.items.find((r) => !r.draft && r.id !== "ID_2024_SPELL_FIREBALL")!;
  const d = await api.homebrewNewDraft({ packId: pack.id, kind: "spell", copyFrom: loaded.id });
  await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: d.draftId });
}

describe("Hy69k: the not-redistributable chip of a homebrew pack", () => {
  it("says the pack holds text copied from imported books, without the local-import words", async () => {
    // Arrange
    const api = mock();
    await packWithCopiedText(api);

    // Act
    renderApp("/content", { api });
    const packs = await screen.findByRole("list", { name: "Content packs" });
    const row = (await within(packs).findByText("Ember Isles")).closest("li")!;

    // Assert
    expect(within(row).getByText("Holds text copied from your imported books — not redistributable")).toBeTruthy();
    expect(within(row).queryByText(/Local import/)).toBeNull();
  });

  it("keeps the local-import chip for a converted pack", async () => {
    // Arrange
    const api = mock();
    const [one] = await api.listPacks();
    api.listPacks = async () => [{ ...one!, id: "local", name: "Converted Pack", kind: "local-import", redistributable: false }];

    // Act
    renderApp("/content", { api });
    const packs = await screen.findByRole("list", { name: "Content packs" });
    const row = (await within(packs).findByText("Converted Pack")).closest("li")!;

    // Assert
    expect(within(row).getByText("Local import — not redistributable")).toBeTruthy();
  });
});
