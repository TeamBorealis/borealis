import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp } from "./utils";

// B-221 (decision U17b): the Obsidian notes view on iPhone, iPad and Android. The backend side (document picker, Storage
// Access Framework, safe writes) is tested in Rust; here: the entries and the Settings row are shown on phones and tablets,
// the phone layout, and the foreground re-scan that stands in for the desktop watcher.
const VAULT = "/home/me/Documents/Campaign vault";
type User = ReturnType<typeof userEvent.setup>;

const NOTES: Record<string, string> = {
  "Home.md": "# Home\n\nSee [[Harbor Town]].\n",
  "Places/Harbor Town.md": "# Harbor Town\n\nA fishing port. #place\n",
  "Session 1.md": "# Session 1\n\nWe met [[Harbor Town]]'s mayor.\n",
};

async function withVault(opts: { mobile?: boolean; autosave?: boolean } = {}): Promise<MockApi> {
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, mobile: opts.mobile ?? true });
  api.vault.load(NOTES);
  const [pick] = await api.pickPath({ purpose: "vault-dir" });
  await api.setVaultDir({ token: pick!.token });
  api.presetSettings({ autosave: opts.autosave ?? false });
  return api;
}

const files = () => screen.getByRole("tree", { name: "Vault files" });
const row = (name: string | RegExp) => within(files()).getByRole("button", { name });
const reading = () => screen.findByRole("article", { name: "Reading view" });

async function open(user: User, name: string): Promise<HTMLElement> {
  await screen.findByRole("tree", { name: "Vault files" });
  await user.click(row(name));
  return reading();
}

async function edit(user: User, text: string): Promise<void> {
  await user.click(screen.getByRole("tab", { name: "Edit" }));
  const box = await screen.findByRole("textbox", { name: "Note text" });
  await user.clear(box);
  await user.type(box, text);
}

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

describe("B-221 Notes on phones and tablets: entries and the vault setting", () => {
  it("is a START sub-tab, a View menu entry and a More destination on phones", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: await withVault() });
    const sub = await screen.findByRole("navigation", { name: "Start sections" });
    expect(within(sub).getAllByRole("link").map((l) => l.textContent)).toEqual(["Character Collection", "Sources", "Additional Content", "Notes"]);
    await user.click(screen.getByRole("menuitem", { name: "View" }));
    expect(screen.getAllByRole("menuitem").map((m) => m.textContent)).toContain("Notes");
  });

  it("lists Notes on the phone's More screen", async () => {
    renderApp("/more", { api: await withVault() });
    const more = await screen.findByRole("list", { name: "More destinations" });
    expect(within(more).getByRole("link", { name: "Notes" }).getAttribute("href")).toBe("/notes");
  });

  it("opens the vault on a phone instead of going back to the characters", async () => {
    renderApp("/notes", { api: await withVault() });
    expect(await screen.findByRole("tree", { name: "Vault files" })).toBeTruthy();
    expect(row("Home")).toBeTruthy();
  });

  it("Settings > Content has the vault row on phones; Browse opens the system picker and waits for Save", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 1, mobile: true });
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    expect(within(dialog).getByText(/Choose the vault folder itself/)).toBeTruthy();
    const box = () => within(dialog).getByLabelText("Obsidian vault") as HTMLInputElement;
    expect(box().value).toBe("");
    await user.click(within(dialog).getByRole("button", { name: "Browse for obsidian vault" }));
    await waitFor(() => expect(box().value).toBe(VAULT));
    expect((await api.getSettings()).vaultDir).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).vaultDir).toBe(VAULT);
  });

  it("Android has Notes and the Settings row too", async () => {
    setPhoneOsForTests("android");
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    expect(await screen.findByRole("tree", { name: "Vault files" })).toBeTruthy();
    const sub = screen.getByRole("navigation", { name: "Start sections" });
    expect(within(sub).getByRole("link", { name: "Notes" })).toBeTruthy();
    await user.click(screen.getByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    expect(within(dialog).getByLabelText("Obsidian vault")).toBeTruthy();
  });

  it("on Android, where a vault has no trash, Delete asks once: Delete for good?", async () => {
    setPhoneOsForTests("android");
    const user = userEvent.setup();
    const api = await withVault();
    api.vault.trashAvailable = false;
    renderApp("/notes", { api });
    await open(user, "Session 1");
    await user.click(screen.getByRole("button", { name: "Delete" }));
    const sure = await screen.findByRole("dialog", { name: "Delete for good?" });
    expect(within(sure).queryByText(/trash\?/)).toBeNull();
    expect(screen.queryByRole("dialog", { name: "Delete?" })).toBeNull();
    expect(api.vault.files.has("Session 1.md")).toBe(true);
    await user.click(within(sure).getByRole("button", { name: "Delete for good" }));
    await waitFor(() => expect(api.vault.files.has("Session 1.md")).toBe(false));
    expect(api.vault.trashed).toEqual([]);
    expect(await screen.findByText("Session 1 was deleted.")).toBeTruthy();
  });

  it("shows where an Android document tree is, not its URI", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    const dirs = await api.getDataDirs();
    api.getDataDirs = async () => ({ ...dirs, vaultDir: "content://com.android.externalstorage.documents/tree/primary%3ADocuments%2FCampaign", vaultLabel: "Internal storage/Documents/Campaign" });
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    await waitFor(() => expect((within(dialog).getByLabelText("Obsidian vault") as HTMLInputElement).value).toBe("Internal storage/Documents/Campaign"));
  });
  it("a vault chosen in Settings while Notes shows its empty state opens at once", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 1, mobile: true });
    api.vault.load(NOTES);
    renderApp("/notes", { api });
    await user.click(await screen.findByRole("button", { name: "Choose vault folder..." }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("button", { name: "Browse for obsidian vault" }));
    await waitFor(() => expect((within(dialog).getByLabelText("Obsidian vault") as HTMLInputElement).value).toBe(VAULT));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("tree", { name: "Vault files" })).toBeTruthy();
  });
});

describe("B-221 Notes on phones: layout", () => {
  it("shows the files, then the open note with a Files button back to them", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const { container } = renderApp("/notes", { api: await withVault() });
    const page = () => container.querySelector(".nt-page")!;
    await screen.findByRole("tree", { name: "Vault files" });
    expect(page().classList.contains("has-note")).toBe(false);
    await open(user, "Home");
    expect(page().classList.contains("has-note")).toBe(true);
    await user.click(screen.getByRole("button", { name: "Files" }));
    await waitFor(() => expect(screen.queryByRole("article", { name: "Reading view" })).toBeNull());
    expect(page().classList.contains("has-note")).toBe(false);
  });

  it("an iPad in the wide shell keeps the three panes and has no Files button", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await open(user, "Home");
    expect(screen.queryByRole("button", { name: "Files" })).toBeNull();
    expect(screen.getByRole("tree", { name: "Vault files" })).toBeTruthy();
  });

  it("has no Attach file on phones (it needs a desktop file picker)", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await open(user, "Home");
    await user.click(screen.getByRole("tab", { name: "Edit" }));
    await screen.findByRole("textbox", { name: "Note text" });
    expect(screen.getByRole("button", { name: "Insert link" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Attach file" })).toBeNull();
  });
});

describe("B-221 Notes on phones: changes made while the app was away", () => {
  it("has no watcher; coming back to the foreground reads the vault and reloads a note without edits", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await open(user, "Home");
    expect(api.vault.watching).toBe(false);
    // Obsidian Sync changes the note while Borealis is in the background, and adds a note.
    api.appLifecycle(false);
    api.vault.externalWrite("Home.md", "# Home\n\nSynced from another device.\n");
    api.vault.externalWrite("New from sync.md", "# New\n");
    api.appLifecycle(true);
    expect(await screen.findByText("Synced from another device.")).toBeTruthy();
    expect(screen.getByText("This note changed in another app and was reloaded.")).toBeTruthy();
    await waitFor(() => expect(row("New from sync")).toBeTruthy());
  });

  it("asks before overwriting when the note changed under unsaved edits", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await open(user, "Home");
    await edit(user, "My edit");
    api.vault.externalWrite("Home.md", "Theirs\n");
    // Leaving the app saves the open note; the write is refused and the user is asked, as on desktop.
    api.appLifecycle(false);
    const dialog = await screen.findByRole("dialog", { name: /changed in another app/i });
    expect(api.vault.files.get("Home.md")?.text).toBe("Theirs\n");
    await user.click(within(dialog).getByRole("button", { name: "Keep mine" }));
    await waitFor(() => expect(api.vault.files.get("Home.md")?.text).toBe("My edit"));
  });

  it("coming back with unsaved edits and a changed note asks too", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await open(user, "Home");
    await edit(user, "Mine");
    api.vault.externalWrite("Home.md", "Theirs\n");
    // Only the return is reported (an OS that sends no background event): the re-check finds the conflict.
    api.appLifecycle(false);
    await screen.findByRole("dialog", { name: /changed in another app/i });
    expect(api.vault.files.get("Home.md")?.text).toBe("Theirs\n");
  });

  it("saves the open note when the app goes to the background", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await open(user, "Home");
    await edit(user, "Written before leaving");
    expect(api.vault.files.get("Home.md")?.text).toBe(NOTES["Home.md"]);
    api.appLifecycle(false);
    await waitFor(() => expect(api.vault.files.get("Home.md")?.text).toBe("Written before leaving"));
  });

  it("desktop keeps its watcher and ignores the phone lifecycle", async () => {
    const user = userEvent.setup();
    const api = await withVault({ mobile: false });
    renderApp("/notes", { api });
    await open(user, "Home");
    await waitFor(() => expect(api.vault.watching).toBe(true));
    const reads = api.vault.opened.length;
    api.appLifecycle(false);
    api.appLifecycle(true);
    await new Promise((r) => setTimeout(r, 50));
    expect(api.vault.opened.length).toBe(reads);
  });
});

describe("B-221 Notes on iPhone and iPad: iCloud Drive files not downloaded yet", () => {
  it("lists them as not downloaded, says so when opened, and opens the note once it is there", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    api.vault.cloud.add("Session 1.md");
    renderApp("/notes", { api });
    await screen.findByRole("tree", { name: "Vault files" });
    await user.click(row(/^Session 1\s*, not downloaded yet$/));
    expect(await screen.findByText(/still downloading from iCloud/)).toBeTruthy();
    api.vault.cloud.delete("Session 1.md");
    expect(await screen.findByRole("article", { name: "Reading view" }, { timeout: 5000 })).toBeTruthy();
    expect(screen.getByText(/mayor/)).toBeTruthy();
  });
});
