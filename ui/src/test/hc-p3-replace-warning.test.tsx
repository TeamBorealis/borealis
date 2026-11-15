import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { base64ToFile } from "../lib/files";
import { renderApp } from "./utils";

// P3 H1OPU (HC11, spec 10.3): Replace of an installed pack with a file that lacks elements local characters use asks
// first, in plain words, naming the characters and the elements; Cancel keeps the installed pack, Replace goes on.

const mock = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });
type User = ReturnType<typeof userEvent.setup>;

/** The pack's file, previewed over the installed pack; the backend's scan (mocked here) found Mira using Ember Bolt. */
async function previewOverInstalled(api: MockApi, user: User, usedBy: boolean): Promise<HTMLElement> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const d = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, name: "Ember Bolt", school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: d.draftId });
  const file = await api.homebrewExportPack({ packId: pack.id, leaveOutCopiedText: true });
  const real = api.previewImportPack.bind(api);
  vi.spyOn(api, "previewImportPack").mockImplementation(async (args) => {
    const p = await real(args);
    return usedBy && p.homebrew
      ? { ...p, homebrew: { ...p.homebrew, replaceUsedBy: [{ characterId: "c1", name: "Mira" }], replaceDrops: ["Ember Bolt"] } }
      : p;
  });
  renderApp("/homebrew", { api });
  await user.upload(await screen.findByLabelText("Choose a pack file"), base64ToFile(file.fileName, file.dataBase64!));
  return screen.findByRole("dialog", { name: "Import preview" });
}

describe("H1OPU: Replace warns when local characters use elements the file lacks", () => {
  it("asks first, names the characters and elements, and Cancel installs nothing", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const install = vi.spyOn(api, "installImportPreview");
    const preview = await previewOverInstalled(api, user, true);

    // Act
    await user.click(within(preview).getByRole("button", { name: "Replace" }));
    const ask = await screen.findByRole("dialog", { name: "Replace the pack?" });

    // Assert
    expect(within(ask).getByText(/This file doesn't have Ember Bolt, which Mira uses\./)).toBeTruthy();
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Replace the pack?" })).toBeNull());
    expect(install).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Import preview" })).toBeTruthy();
  });

  it("replaces when the user goes on", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const install = vi.spyOn(api, "installImportPreview");
    const preview = await previewOverInstalled(api, user, true);

    // Act
    await user.click(within(preview).getByRole("button", { name: "Replace" }));
    const ask = await screen.findByRole("dialog", { name: "Replace the pack?" });
    await user.click(within(ask).getByRole("button", { name: "Replace" }));

    // Assert
    await waitFor(() => expect(install).toHaveBeenCalledWith(expect.not.objectContaining({ asCopy: true })));
  });

  it("replaces at once when no character loses anything", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const install = vi.spyOn(api, "installImportPreview");
    const preview = await previewOverInstalled(api, user, false);

    // Act
    await user.click(within(preview).getByRole("button", { name: "Replace" }));

    // Assert
    await waitFor(() => expect(install).toHaveBeenCalled());
    expect(screen.queryByRole("dialog", { name: "Replace the pack?" })).toBeNull();
  });
});
