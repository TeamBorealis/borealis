import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { FILLABLE_TOOLTIP } from "../pages/settings/SheetTab";
import { getStatus } from "../state/status";
import { SECONDARY_UNAVAILABLE, describeFolders } from "../components/shell/useFolderNotice";
import { renderApp } from "./utils";

const fast = () => new MockApi({ aiEnabled: true, aiChunkDelayMs: 1, rng: () => 0.5 });

async function openAppSettings(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
  return screen.findByRole("dialog", { name: "Application Settings" });
}

describe("Application Settings window", () => {
  it("has the classic tabs, then the Borealis tabs, on a vertical strip and opens over the current screen", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const dialog = await openAppSettings(user);
    const list = within(dialog).getByRole("tablist", { name: "Settings sections" });
    expect(list.getAttribute("aria-orientation")).toBe("vertical");
    expect(within(list).getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "General", "Appearance", "Character build", "Data files", "Equipment", "Character sheet",
      "Rules", "Saving", "Display", "Content", "Search", "AI assistant", "Devices", "Updates",
      "License", "About",
    ]);
    expect((within(list).getByRole("tab", { name: "General" })).getAttribute("aria-selected")).toBe("true");
    expect(document.querySelector(".aur-set-overlay")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Restore defaults" })).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeTruthy();
  });

  it("opens the CHARACTER SHEET tab for the ?tab=sheet deep link", async () => {
    renderApp("/settings?tab=sheet");
    expect((await screen.findByRole("tab", { name: "Character sheet" })).getAttribute("aria-selected")).toBe("true");
    expect(await screen.findByRole("checkbox", { name: "Form Fillable" })).toBeTruthy();
  });

  it("Cancel discards edits and reverts the live accent preview", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const dialog = await openAppSettings(user);
    const nameBox = await within(dialog).findByLabelText("PLAYER NAME");
    await user.clear(nameBox);
    await user.type(nameBox, "Zed");
    await user.click(within(dialog).getByRole("tab", { name: "Appearance" }));
    await user.click(within(dialog).getByRole("radio", { name: "Red" }));
    expect(document.documentElement.style.getPropertyValue("--accent-base")).toBe("#8e2a2a");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect(document.documentElement.style.getPropertyValue("--accent-base")).toBe("");
    const s = await api.getSettings();
    expect(s.playerName).toBe("Player One");
    expect(s.accentColor).toBe("default");
  });

  it("Save persists the whole form, applies the accent and says so in the status bar", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const dialog = await openAppSettings(user);
    const nameBox = await within(dialog).findByLabelText("PLAYER NAME");
    await user.clear(nameBox);
    await user.type(nameBox, "Zed");
    await user.click(within(dialog).getByRole("checkbox", { name: "Selection Flow" }));
    await user.selectOptions(within(dialog).getByLabelText("ROW HEIGHT"), "large");
    await user.click(within(dialog).getByRole("tab", { name: "Appearance" }));
    await user.click(within(dialog).getByRole("radio", { name: "Blue" }));
    await user.click(within(dialog).getByRole("tab", { name: "Character sheet" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "Attack Cards" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "Start Attack Cards on a New Page" }));
    expect((await api.getSettings()).playerName).toBe("Player One");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    const s = await api.getSettings();
    expect(s).toMatchObject({ playerName: "Zed", selectionFlow: true, rowHeight: "large", accentColor: "blue" });
    expect(s.sheet.cards.attack).toBe(true);
    expect(s.sheet.cardsNewPage.attack).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--accent-base")).toBe("#2a4f8a");
    expect(getStatus()).toBe("Your preferences have been saved.");
  });

  it("Restore defaults saves at once, repopulates the form and stays open", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await api.saveSettings({ patch: { playerName: "Old", portraitSize: 175, accentColor: "pink" } });
    const dialog = await openAppSettings(user);
    expect(((await within(dialog).findByLabelText("PLAYER NAME")) as HTMLInputElement).value).toBe("Old");
    await user.click(within(dialog).getByRole("button", { name: "Restore defaults" }));
    await waitFor(async () => expect((await api.getSettings()).playerName).toBe("Player One"));
    expect((await api.getSettings()).accentColor).toBe("default");
    expect(((await within(dialog).findByLabelText("PLAYER NAME")) as HTMLInputElement).value).toBe("Player One");
    expect(screen.getByRole("dialog", { name: "Application Settings" })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect((await api.getSettings()).playerName).toBe("Player One");
  });

  it("DATA FILES: the documents directory is chosen through the picked-token flow and saved on Save", async () => {
    const user = userEvent.setup();
    const api = fast();
    const purposes: string[] = [];
    const pickPath = api.pickPath.bind(api);
    api.pickPath = async (a) => { purposes.push(a.purpose); return pickPath(a); };
    api.picks.next = "/home/me/Documents/Chars";
    renderApp("/characters", { api });
    const dialog = await openAppSettings(user);
    await user.click(await within(dialog).findByRole("tab", { name: "Data files" }));
    await user.click(await within(dialog).findByRole("button", { name: "Browse for documents directory" }));
    await waitFor(() => expect((within(dialog).getByLabelText("Documents directory") as HTMLInputElement).value).toBe("/home/me/Documents/Chars"));
    expect(purposes).toEqual(["documents-dir"]);
    expect((await api.getDataDirs()).isDefault).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(async () => expect((await api.getDataDirs()).documentsDir).toBe("/home/me/Documents/Chars"));
    expect((await api.getSettings()).documentsDir).toBe("/home/me/Documents/Chars");
  });

  it("DATA FILES: unavailable folders keep their paths and show a warning, also in the status bar at startup", async () => {
    const user = userEvent.setup();
    const api = fast();
    const getDataDirs = api.getDataDirs.bind(api);
    api.getDataDirs = async () => ({
      ...(await getDataDirs()), documentsDirUnavailable: "/Volumes/Drive/Chars",
      secondaryContentDir: "/Volumes/Drive/Homebrew", secondaryContentUnavailable: true,
    });
    renderApp("/characters", { api });
    await waitFor(() => expect(getStatus()).toBe(describeFolders({
      documentsDir: "", defaultDocumentsDir: "", isDefault: true, charactersDir: "",
      documentsDirUnavailable: "/Volumes/Drive/Chars", secondaryContentDir: null, secondaryContentUnavailable: true, portraitLibraryDir: null, portraitLibraryUnavailable: false,
    })));
    expect(getStatus()).toContain("/Volumes/Drive/Chars");
    expect(getStatus()).toContain(SECONDARY_UNAVAILABLE);
    const dialog = await openAppSettings(user);
    await user.click(await within(dialog).findByRole("tab", { name: "Data files" }));
    expect((within(dialog).getByLabelText("Documents directory") as HTMLInputElement).value).toBe("/Volumes/Drive/Chars");
    expect((within(dialog).getByLabelText("Secondary content directory") as HTMLInputElement).value).toBe("/Volumes/Drive/Homebrew");
    const notes = within(dialog).getAllByRole("status").map((n) => n.textContent ?? "");
    expect(notes.some((t) => t.includes("documents directory /Volumes/Drive/Chars is not available"))).toBe(true);
    expect(notes.some((t) => t.includes("secondary content directory is not available"))).toBe(true);
    expect((within(dialog).getByRole("button", { name: /Unlink secondary directory/i }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("describeFolders says nothing when every folder is reachable", () => {
    expect(describeFolders({
      documentsDir: "/d", defaultDocumentsDir: "/d", isDefault: true, charactersDir: "/d/characters",
      documentsDirUnavailable: null, secondaryContentDir: "/s", secondaryContentUnavailable: false, portraitLibraryDir: "/p", portraitLibraryUnavailable: false,
    })).toBeNull();
  });

  it("DATA FILES: secondary directory browse and unlink, automatic updates, Clear Cache disables itself", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/characters", { api });
    const dialog = await openAppSettings(user);
    await user.click(await within(dialog).findByRole("tab", { name: "Data files" }));
    const unlink = within(dialog).getByRole("button", { name: /Unlink secondary directory/i });
    expect((unlink as HTMLButtonElement).disabled).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Browse for secondary content directory" }));
    await waitFor(() => expect((unlink as HTMLButtonElement).disabled).toBe(false));
    await user.click(within(dialog).getByRole("checkbox", { name: "Automatic Updates" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(async () => expect((await api.getSettings()).secondaryContentDir).toBe("/home/me/Documents/Custom Content"));
    expect((await api.getSettings()).autoContentUpdate).toBe(false);
    const again = await openAppSettings(user);
    await user.click(await within(again).findByRole("tab", { name: "Data files" }));
    await user.click(within(again).getByRole("button", { name: /Unlink secondary directory/i }));
    await user.click(within(again).getByRole("button", { name: "Save" }));
    await waitFor(async () => expect((await api.getSettings()).secondaryContentDir).toBeNull());
    const third = await openAppSettings(user);
    await user.click(await within(third).findByRole("tab", { name: "Data files" }));
    const clear = within(third).getByRole("button", { name: "Clear Cache" });
    await user.click(clear);
    await waitFor(() => expect((clear as HTMLButtonElement).disabled).toBe(true));
  });
});

describe("Content update notice", () => {
  it("shows the startup auto-update result in the status bar", async () => {
    const api = fast();
    Object.assign(api, { contentUpdateStatus: async () => ({ state: "done", message: "Your content files have been updated.", updated: 2, failed: [], checkedAt: "2026-10-11T09:00:00Z" }) });
    renderApp("/characters", { api });
    expect(await screen.findByText("Your content files have been updated and the new content is loaded.")).toBeTruthy();
  });
});

describe("Character sheet theme and edition options", () => {
  it("render with defaults, persist on Save, and old settings without the keys load with defaults", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const dialog = await openAppSettings(user);
    await user.click(within(dialog).getByRole("tab", { name: "Character sheet" }));
    const theme = within(dialog).getByLabelText("THEME") as HTMLSelectElement;
    const edition = within(dialog).getByLabelText("RULES EDITION") as HTMLSelectElement;
    expect(theme.value).toBe("colour");
    expect(edition.value).toBe("auto");
    expect((within(dialog).getByRole("checkbox", { name: "Form Fillable" }) as HTMLInputElement).checked).toBe(false);
    expect(within(dialog).getByRole("checkbox", { name: "Form Fillable" }).closest(".aur-set-check")?.getAttribute("title")).toBe(FILLABLE_TOOLTIP);
    await user.selectOptions(theme, "ink");
    await user.selectOptions(edition, "2024");
    expect((await api.getSettings()).sheet.theme).toBe("colour");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    const s = (await api.getSettings()).sheet;
    expect(s).toMatchObject({ theme: "ink", edition: "2024", formFillable: false });
  });

  it("MockApi supplies the new keys by default", async () => {
    const s = (await fast().getSettings()).sheet;
    expect(s).toMatchObject({ theme: "colour", edition: "auto", formFillable: false });
  });
});
