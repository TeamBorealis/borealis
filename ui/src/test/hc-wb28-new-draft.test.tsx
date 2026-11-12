import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

// Board item IDpe8 (spec 10.1, "drafts survive restarts"): a new element that was never saved keeps its draft on the
// device, and the pack's list offers it again (after leaving the editor, or after a restart: the workspace opens with
// nothing in the editor and the draft only on disk).

const mock = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });
const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;

async function packWithDrafts(api: MockApi): Promise<{ packId: string; draftId: string }> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const saved = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  await api.homebrewUpdateDraft({ draftId: saved.draftId, form: { ...saved.form, fields: { ...saved.form.fields, name: "Ember Bolt", school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: saved.draftId });
  const fresh = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, name: "Half Done", school: "Abjuration" } } });
  return { packId: pack.id, draftId: fresh.draftId };
}

describe("a never-saved element's draft (IDpe8, spec 10.1)", () => {
  it("is listed in its pack and Keep editing resumes it", async () => {
    // Arrange: the draft is on the device, nothing is open (a restart)
    const user = userEvent.setup();
    const api = mock();
    const { packId } = await packWithDrafts(api);
    renderApp(`/homebrew/${packId}`, { api });

    // Act
    const row = await screen.findByRole("button", { name: "Half Done" });
    expect(row.closest("tr")?.querySelector('[title="Not saved yet"]')).toBeTruthy();
    await user.click(row);
    const box = await screen.findByRole("dialog", { name: "Unsaved changes" });
    expect(box.textContent).toMatch(/Unsaved changes from /);
    await user.click(within(box).getByRole("button", { name: "Keep editing" }));

    // Assert
    await waitFor(() => expect(nameBox().value).toBe("Half Done"));
  });

  it("stays in the list after the editor moves to another element", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const { packId } = await packWithDrafts(api);
    renderApp(`/homebrew/${packId}`, { api });
    await user.click(await screen.findByRole("button", { name: "Half Done" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Unsaved changes" })).getByRole("button", { name: "Keep editing" }));
    await waitFor(() => expect(nameBox().value).toBe("Half Done"));

    // Act
    await user.click(screen.getByRole("button", { name: "Ember Bolt" }));
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));

    // Assert
    expect(await screen.findByRole("button", { name: "Half Done" })).toBeTruthy();
  });

  it("Discard drops it from the device and the list", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const { packId } = await packWithDrafts(api);
    renderApp(`/homebrew/${packId}`, { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Half Done" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Unsaved changes" })).getByRole("button", { name: "Discard" }));

    // Assert
    await waitFor(() => expect(screen.queryByRole("button", { name: "Half Done" })).toBeNull());
    expect(await api.homebrewListDrafts()).toEqual([]);
  });

  it("belongs to its own kind and pack only", async () => {
    // Arrange: a feat draft in this pack, a spell draft in another pack
    const api = mock();
    const { packId } = await packWithDrafts(api);
    const feat = await api.homebrewNewDraft({ packId, kind: "feat" });
    await api.homebrewUpdateDraft({ draftId: feat.draftId, form: { ...feat.form, fields: { ...feat.form.fields, name: "Quick Study" } } });
    const other = await api.homebrewCreatePack({ request: { name: "Grey Coast", code: "GRY", edition: "dnd2024" } });
    const elsewhere = await api.homebrewNewDraft({ packId: other.id, kind: "spell" });
    await api.homebrewUpdateDraft({ draftId: elsewhere.draftId, form: { ...elsewhere.form, fields: { ...elsewhere.form.fields, name: "Far Away" } } });

    // Act
    renderApp(`/homebrew/${packId}`, { api });
    await screen.findByRole("button", { name: "Half Done" });

    // Assert: the spell list shows only this pack's spell draft
    expect(screen.queryByRole("button", { name: "Quick Study" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Far Away" })).toBeNull();
  });
});
