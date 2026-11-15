import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ApiEvents } from "../api/contract";
import { MockApi } from "../api/mock";
import { base64ToFile } from "../lib/files";
import { copiedTextLine } from "../pages/homebrew/ExportPackDialog";
import { renderApp } from "./utils";

// Homebrew WB-24 (spec 14.2, 14.4, 10.1; HC7, HC8) on the mock: Export pack (the copied-text sentence and "Leave that
// text out", default on), importing the file back (Replace or Import as a copy when the pack is installed), and the
// "Element changed" prompt when a received pack changes under an open draft.

const mock = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });
type User = ReturnType<typeof userEvent.setup>;

async function packWithSpell(api: MockApi, name = "Ember Bolt"): Promise<string> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const d = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, name, school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: d.draftId });
  return pack.id;
}

/** A copy of a loaded spell that is not SRD content (the mock's stand-in for text copied from an imported book). */
async function addCopiedSpell(api: MockApi, packId: string): Promise<void> {
  const rows = await api.homebrewReferenceOptions({ target: { types: ["Spell"] }, query: "", filters: { srdOnly: false }, page: { offset: 0, limit: 50 } });
  const loaded = rows.items.find((r) => !r.draft && r.id !== "ID_2024_SPELL_FIREBALL")!;
  const d = await api.homebrewNewDraft({ packId, kind: "spell", copyFrom: loaded.id });
  await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: d.draftId });
}

async function openExport(user: User): Promise<HTMLElement> {
  await user.click(await screen.findByRole("row", { name: /Ember Isles/ }));
  await user.click(screen.getByRole("button", { name: "Export pack" }));
  return screen.findByRole("dialog", { name: "Export pack" });
}

describe("WB-24 export pack (spec 14.2, HC8)", () => {
  // the desktop browser download: a link to the bytes, clicked (jsdom does not navigate)
  let downloads: string[] = [];
  beforeEach(() => {
    downloads = [];
    URL.createObjectURL = vi.fn(() => "blob:pack");
    URL.revokeObjectURL = vi.fn();
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) { downloads.push(this.download); });
  });
  afterEach(() => vi.restoreAllMocks());

  it("says how many elements hold copied text and leaves it out by default", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    await addCopiedSpell(api, packId);
    const exported = vi.spyOn(api, "homebrewExportPack");
    renderApp("/homebrew", { api });

    // Act
    const dialog = await openExport(user);
    const leaveOut = await within(dialog).findByRole("checkbox", { name: /Leave that text out/ }) as HTMLInputElement;

    // Assert
    expect(within(dialog).getByText(copiedTextLine(1))).toBeTruthy();
    expect(copiedTextLine(3)).toBe("3 elements contain text copied from your imported books. That text isn't yours to share.");
    expect(leaveOut.checked).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Export" }));
    await waitFor(() => expect(exported).toHaveBeenCalledWith({ packId, leaveOutCopiedText: true }));
    expect(await screen.findByText("Saved Ember_Isles.borealis-pack.zip.")).toBeTruthy();
    expect(downloads).toEqual(["Ember_Isles.borealis-pack.zip"]);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Export pack" })).toBeNull());
  });

  it("keeps the text when the box is cleared, and asks nothing about text for a pack without copies", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const exported = vi.spyOn(api, "homebrewExportPack");
    renderApp("/homebrew", { api });

    // Act
    const dialog = await openExport(user);
    await waitFor(() => expect((within(dialog).getByRole("button", { name: "Export" }) as HTMLButtonElement).disabled).toBe(false));

    // Assert
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    await addCopiedSpell(api, packId);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    const again = await openExport(user);
    await user.click(await within(again).findByRole("checkbox", { name: /Leave that text out/ }));
    await user.click(within(again).getByRole("button", { name: "Export" }));
    await waitFor(() => expect(exported).toHaveBeenCalledWith({ packId, leaveOutCopiedText: false }));
  });
});

describe("WB-24 import a pack file (spec 14.2)", () => {
  it("offers Replace or Import as a copy for an installed pack; the copy opens in the creator", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const file = await api.homebrewExportPack({ packId, leaveOutCopiedText: true });
    renderApp("/homebrew", { api });

    // Act
    await user.upload(await screen.findByLabelText("Choose a pack file"), base64ToFile(file.fileName, file.dataBase64!));
    const preview = await screen.findByRole("dialog", { name: "Import preview" });

    // Assert
    expect(within(preview).getByText(/You already have the pack "Ember Isles"/)).toBeTruthy();
    expect(within(preview).getByRole("button", { name: "Replace" })).toBeTruthy();
    expect(within(preview).queryByRole("button", { name: "Install" })).toBeNull();
    await user.click(within(preview).getByRole("button", { name: "Import as a copy" }));
    expect(await screen.findByRole("heading", { name: "Homebrew: Ember Isles (copy)" })).toBeTruthy();
    expect((await api.homebrewListElements({ packId: "homebrew-emb2" })).map((r) => r.id)).toEqual(["ID_EMB2_SPELL_EMBER_BOLT"]);
    expect((await api.homebrewListElements({ packId })).map((r) => r.id)).toEqual(["ID_EMB_SPELL_EMBER_BOLT"]);
  });

  it("installs a pack that is not here as it is", async () => {
    // Arrange
    const user = userEvent.setup();
    const source = mock();
    const packId = await packWithSpell(source);
    const file = await source.homebrewExportPack({ packId, leaveOutCopiedText: true });
    const api = mock();
    renderApp("/homebrew", { api });

    // Act
    await user.upload(await screen.findByLabelText("Choose a pack file"), base64ToFile(file.fileName, file.dataBase64!));
    const preview = await screen.findByRole("dialog", { name: "Import preview" });
    await user.click(within(preview).getByRole("button", { name: "Install" }));

    // Assert
    expect(await screen.findByRole("heading", { name: "Homebrew: Ember Isles" })).toBeTruthy();
    expect((await api.homebrewListElements({ packId })).map((r) => r.name)).toEqual(["Ember Bolt"]);
  });
});

describe("WB-24 a pack received under an open draft (spec 14.4, 10.1)", () => {
  it("asks Keep my draft / Use the saved version", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const reloaded: ((p: ApiEvents["content:reloaded"]) => void)[] = [];
    const subscribe = api.subscribe.bind(api);
    type Reloaded = (p: ApiEvents["content:reloaded"]) => void;
    vi.spyOn(api, "subscribe").mockImplementation(((event: string, handler: Reloaded) => {
      if (event === "content:reloaded") reloaded.push(handler);
      return subscribe(event as "content:reloaded", handler);
    }) as typeof api.subscribe);
    renderApp(`/homebrew/${packId}`, { api });
    await user.click(await screen.findByRole("button", { name: "Ember Bolt" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Ember Bolt"));
    const real = api.homebrewOpenDraft.bind(api);
    vi.spyOn(api, "homebrewOpenDraft").mockImplementation(async (args) => ({ ...(await real(args)), baseChanged: true }));

    // Act
    act(() => reloaded.forEach((h) => h({ reason: "device-sync" })));

    // Assert
    const box = await screen.findByRole("dialog", { name: "Element changed" });
    expect(within(box).getByText("This element changed since you started editing.")).toBeTruthy();
    expect(within(box).getByRole("button", { name: "Use the saved version" })).toBeTruthy();
  });
});
