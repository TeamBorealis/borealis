import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { VAULT_UNAVAILABLE, describeFolders } from "../components/shell/useFolderNotice";
import { renderApp } from "./utils";

// GitHub #15 (B-102): the notes view over an Obsidian vault, against the repo's fixture vault (original text).
const FIXTURE = join(__dirname, "../../../src-tauri/tests/fixtures/vault");
const VAULT = "/home/me/Documents/Campaign vault";
type User = ReturnType<typeof userEvent.setup>;

/** The fixture vault's visible files: text as text, images as `data:` URLs; not-UTF-8 files are left out. */
function fixtureFiles(): Record<string, string> {
  const out: Record<string, string> = {};
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      if (name.startsWith(".")) continue;
      const full = join(dir, name);
      if (statSync(full).isDirectory()) { walk(full); continue; }
      const rel = relative(FIXTURE, full).split("\\").join("/");
      const bytes = readFileSync(full);
      if (/\.png$/i.test(name)) { out[rel] = `data:image/png;base64,${bytes.toString("base64")}`; continue; }
      try { out[rel] = new TextDecoder("utf-8", { fatal: true }).decode(bytes); } catch { /* not UTF-8 */ }
    }
  };
  walk(FIXTURE);
  return out;
}

/** A mock backend with the fixture vault open (its `.obsidian/app.json` too). */
async function withVault(opts: { autosave?: boolean; mobile?: boolean; macAppStore?: boolean } = {}): Promise<MockApi> {
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, mobile: opts.mobile, macAppStore: opts.macAppStore });
  api.vault.load(fixtureFiles());
  api.vault.config = { ...api.vault.config, ...JSON.parse(readFileSync(join(FIXTURE, ".obsidian/app.json"), "utf8")) };
  const [pick] = await api.pickPath({ purpose: "vault-dir" });
  await api.setVaultDir({ token: pick!.token });
  api.presetSettings({ autosave: opts.autosave ?? false });
  return api;
}

const files = () => screen.getByRole("tree", { name: "Vault files" });
const row = (name: string) => within(files()).getByRole("button", { name });
const text = (api: MockApi, path: string) => api.vault.files.get(path)?.text;

async function openNote(user: User, ...names: string[]): Promise<HTMLElement> {
  await screen.findByRole("tree", { name: "Vault files" });
  for (const n of names) await user.click(row(n));
  return screen.findByRole("article", { name: "Reading view" });
}

async function editMode(user: User): Promise<HTMLTextAreaElement> {
  await user.click(screen.getByRole("tab", { name: "Edit" }));
  return screen.findByRole("textbox", { name: "Note text" }) as Promise<HTMLTextAreaElement>;
}

describe("B-102 Notes: navigation and the vault setting", () => {
  it("is a START sub-tab and a View menu entry after the classic builder's own items, on desktop only", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: await withVault() });
    const sub = await screen.findByRole("navigation", { name: "Start sections" });
    expect(within(sub).getAllByRole("link").map((l) => l.textContent)).toEqual(["Character Collection", "Sources", "Additional Content", "Notes"]);
    await user.click(screen.getByRole("menuitem", { name: "View" }));
    const items = screen.getAllByRole("menuitem").map((m) => m.textContent);
    expect(items.indexOf("Notes")).toBe(items.indexOf("Additional Content") + 1);
  });

  it("is hidden on phones, and its route goes back to the characters", async () => {
    renderApp("/notes", { api: await withVault({ mobile: true }) });
    await waitFor(() => expect(screen.queryByRole("tree", { name: "Vault files" })).toBeNull());
    expect(screen.queryByRole("link", { name: "Notes" })).toBeNull();
  });

  it("without a vault says where to choose one", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: new MockApi({ aiChunkDelayMs: 1 }) });
    await user.click(await screen.findByRole("button", { name: "Choose vault folder..." }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    expect(within(dialog).getByLabelText("Obsidian vault")).toBeTruthy();
  });

  it("Settings > Content: Browse waits for Save, Clear turns it off, an unplugged vault keeps the path", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 1 });
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    let dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    const box = () => within(dialog).getByLabelText("Obsidian vault") as HTMLInputElement;
    expect(box().value).toBe("");
    await user.click(within(dialog).getByRole("button", { name: "Browse for obsidian vault" }));
    await waitFor(() => expect(box().value).toBe(VAULT));
    expect((await api.getSettings()).vaultDir).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).vaultDir).toBe(VAULT);

    api.vault.unavailable = true;
    await user.click(screen.getByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    await waitFor(() => expect(within(dialog).getByText(new RegExp(VAULT_UNAVAILABLE.slice(0, 30)))).toBeTruthy());
    expect(box().value).toBe(VAULT);
    await user.click(within(dialog).getByRole("button", { name: "Clear vault" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).vaultDir).toBeNull();
    expect(describeFolders({ ...(await api.getDataDirs()), vaultUnavailable: true })).toContain(VAULT_UNAVAILABLE);
  });

  it("the Mac App Store build has the Notes section and the vault row", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault({ macAppStore: true }) });
    expect(await screen.findByRole("tree", { name: "Vault files" })).toBeTruthy();
    await user.click(screen.getByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    expect((within(dialog).getByLabelText("Obsidian vault") as HTMLInputElement).value).toBe(VAULT);
  });
});

describe("B-102 Notes: tree, reading and editing", () => {
  it("shows the folder tree with every visible file, folders first", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await screen.findByRole("tree", { name: "Vault files" });
    const top = within(files()).getAllByRole("treeitem").map((t) => t.querySelector(".nt-name")?.textContent);
    expect(top).toEqual(["Archive", "Handouts", "Odd", "People", "Places", "Board.canvas", "Home", "map.png", "Session 1"]);
    await user.click(row("Handouts"));
    expect(row("Ferry schedule.pdf")).toBeTruthy();
    expect(within(files()).queryByText(/hidden|Old idea|app\.json/)).toBeNull();
  });

  it("opens a note in the reading view with its properties, embeds and callouts", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    const read = await openNote(user, "Home");
    expect(within(read).getByRole("heading", { level: 1, name: "The Copper Coast campaign" })).toBeTruthy();
    const props = within(read).getByRole("region", { name: "Properties" });
    expect(within(props).getByText("status")).toBeTruthy();
    expect(within(props).getByRole("button", { name: "#campaign" })).toBeTruthy();
    await waitFor(() => expect(read.querySelector("img.md-embed-image")?.getAttribute("src")).toMatch(/^data:image\/png/));
    await waitFor(() => expect(read.querySelector(".md-embed .md-embed-body")?.textContent).toContain("Copper goggles"));
    expect(read.querySelector('details[data-callout="warning"]')).toBeTruthy();
    expect(read.querySelector("pre code.language-dataview")).toBeTruthy();
    await waitFor(() => expect(within(read).getByRole("link", { name: "Missing Note" }).classList.contains("is-unresolved")).toBe(true));
  });

  it("edits the raw text, front matter included, and saves with Ctrl+S", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "People", "Bramblewick");
    const area = await editMode(user);
    expect(area.value).toBe(text(api, "People/Bramblewick.md"));
    await user.click(area);
    await user.keyboard("{Control>}{End}{/Control}Met again in session two.");
    expect(screen.getByText("Unsaved changes")).toBeTruthy();
    expect(text(api, "People/Bramblewick.md")).not.toContain("session two");
    fireEvent.keyDown(area, { key: "s", ctrlKey: true });
    await waitFor(() => expect(text(api, "People/Bramblewick.md")).toContain("Met again in session two."));
    expect(screen.queryByText("Unsaved changes")).toBeNull();
    expect(screen.queryByRole("dialog", { name: /Save/ })).toBeNull();
  });

  it("autosaves after a pause when autosave is on, and never writes without a change", async () => {
    const user = userEvent.setup();
    const api = await withVault({ autosave: true });
    const before = text(api, "Places/Harbor Town.md");
    renderApp("/notes", { api });
    await openNote(user, "Places", "Harbor Town");
    await new Promise((r) => setTimeout(r, 1700));
    expect(text(api, "Places/Harbor Town.md")).toBe(before);
    const area = await editMode(user);
    await user.click(area);
    await user.keyboard("{Control>}{End}{/Control}!");
    await waitFor(() => expect(text(api, "Places/Harbor Town.md")).toBe(`${before}!`), { timeout: 4000 });
  });

  it("ticks a task in the raw text when its checkbox is clicked", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    const before = text(api, "Home.md")!;
    renderApp("/notes", { api });
    const read = await openNote(user, "Home");
    const box = read.querySelectorAll<HTMLInputElement>("input.task-checkbox")[0]!;
    await user.click(box);
    await user.click(screen.getByRole("button", { name: "Save note" }));
    await waitFor(() => expect(text(api, "Home.md")).toBe(before.replace("- [ ] Write up the lighthouse", "- [x] Write up the lighthouse")));
  });

  it("opens canvases as plain text and other files as listed only", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await screen.findByRole("tree", { name: "Vault files" });
    await user.click(row("Board.canvas"));
    expect((await screen.findByLabelText("File text")).textContent).toContain("Plot board");
    expect(screen.getByText(/shows this file as plain text/)).toBeTruthy();
    await user.click(row("Handouts"));
    await user.click(row("Ferry schedule.pdf"));
    expect(await screen.findByText(/lists this file but does not open it/)).toBeTruthy();
  });
});

describe("B-102 Notes: changes made by other apps", () => {
  it("reloads a note without edits", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    const read = await openNote(user, "Session 1");
    api.vault.externalWrite("Session 1.md", "# Session 1\n\nRewritten in Obsidian.\n");
    await waitFor(() => expect(read.textContent).toContain("Rewritten in Obsidian."));
    expect(screen.getByText("This note changed in another app and was reloaded.")).toBeTruthy();
  });

  it("asks when both changed: show both, then keep mine", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "Session 1");
    const area = await editMode(user);
    await user.click(area);
    await user.keyboard("{Control>}{End}{/Control}Mine.");
    api.vault.externalWrite("Session 1.md", "Theirs.\n");
    const dialog = await screen.findByRole("dialog", { name: "Note changed in another app" });
    expect(text(api, "Session 1.md")).toBe("Theirs.\n");
    await user.click(within(dialog).getByRole("button", { name: "Show both" }));
    expect((within(dialog).getByLabelText("In the file now") as HTMLTextAreaElement).value).toBe("Theirs.\n");
    expect((within(dialog).getByLabelText("Your version") as HTMLTextAreaElement).value).toMatch(/Mine\.$/);
    await user.click(within(dialog).getByRole("button", { name: "Keep mine" }));
    await waitFor(() => expect(text(api, "Session 1.md")).toMatch(/Mine\.$/));
  });

  it("asks when both changed: take theirs drops my edits", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "Session 1");
    const area = await editMode(user);
    await user.click(area);
    await user.keyboard("Mine.");
    api.vault.externalWrite("Session 1.md", "Theirs.\n");
    const dialog = await screen.findByRole("dialog", { name: "Note changed in another app" });
    await user.click(within(dialog).getByRole("button", { name: "Take theirs" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: "Note text" }) as HTMLTextAreaElement).value).toBe("Theirs.\n"));
    expect(text(api, "Session 1.md")).toBe("Theirs.\n");
  });

  it("offers to save a note again when another app deleted it during an edit", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "Session 1");
    const area = await editMode(user);
    await user.click(area);
    await user.keyboard("Kept.");
    api.vault.externalWrite("Session 1.md", null);
    const dialog = await screen.findByRole("dialog", { name: "Note deleted in another app" });
    await user.click(within(dialog).getByRole("button", { name: "Save it again" }));
    await waitFor(() => expect(text(api, "Session 1.md")).toContain("Kept."));
  });
});

describe("B-102 Notes: links, backlinks, tags and search", () => {
  it("follows a wikilink to the note and offers to create an unresolved one", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    const read = await openNote(user, "Home");
    await user.click(within(read).getByRole("link", { name: "the harbor" }));
    expect(await screen.findByRole("region", { name: "Harbor Town" })).toBeTruthy();
    expect(row("Harbor Town").classList.contains("sel")).toBe(true);

    await user.click(row("Home"));
    const home = await screen.findByRole("article", { name: "Reading view" });
    await user.click(await within(home).findByRole("link", { name: "Missing Note" }));
    const ask = await screen.findByRole("dialog", { name: "Create note?" });
    await user.click(within(ask).getByRole("button", { name: "Create note" }));
    await waitFor(() => expect(api.vault.files.has("Inbox/Missing Note.md")).toBe(true));
    expect(await screen.findByRole("textbox", { name: "Note text" })).toBeTruthy();
  });

  it("lists backlinks and the vault's tags in the description pane", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await openNote(user, "People", "Bramblewick");
    const pane = screen.getByRole("complementary", { name: "Note details" });
    const back = await within(pane).findByRole("list", { name: "Backlinks" });
    await waitFor(() => expect(within(back).getAllByRole("button").filter((b) => b.classList.contains("nt-link")).map((b) => b.textContent)).toEqual(["Home", "Places/Harbor Town", "Session 1"]));
    await user.click(within(back).getByRole("button", { name: "Session 1" }));
    expect(await screen.findByRole("region", { name: "Session 1" })).toBeTruthy();
    const tags = within(pane).getByRole("list", { name: "Tags" });
    await user.click(within(tags).getByRole("button", { name: (n) => n.startsWith("#npc") && !n.includes("/") }));
    const results = await screen.findByRole("list", { name: "Search results" });
    expect(screen.getByText("Notes tagged #npc")).toBeTruthy();
    expect(within(results).getAllByRole("button").map((b) => b.textContent)).toEqual(["People/Bramblewick", "Session 1"]);
  });

  it("searches note names and text", async () => {
    const user = userEvent.setup();
    renderApp("/notes", { api: await withVault() });
    await screen.findByRole("tree", { name: "Vault files" });
    await user.type(screen.getByRole("searchbox", { name: "Search notes" }), "lighthouse");
    const results = await screen.findByRole("list", { name: "Search results" });
    expect(within(results).getAllByRole("button").map((b) => b.textContent)).toEqual(["Home", "Places/Harbor Town"]);
  });

  it("opens web links through the backend", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    const read = await openNote(user, "Home");
    await user.click(within(read).getByRole("link", { name: "the rules" }));
    await waitFor(() => expect(api.openedUrls).toEqual(["https://example.org/rules"]));
  });
});

describe("B-102 Notes: create, rename and delete", () => {
  it("creates a note where .obsidian/app.json says", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await screen.findByRole("tree", { name: "Vault files" });
    await user.click(screen.getByRole("button", { name: "New note" }));
    await waitFor(() => expect(api.vault.files.has("Inbox/Untitled.md")).toBe(true));
    expect(await screen.findByRole("textbox", { name: "Note text" })).toBeTruthy();
  });

  it("shows the link changes and updates links only when confirmed", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "People", "Bramblewick");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    const name = await screen.findByRole("dialog", { name: "Rename" });
    const input = within(name).getByLabelText("New name");
    await user.clear(input);
    await user.type(input, "Bram");
    await user.click(within(name).getByRole("button", { name: "Rename" }));
    const links = await screen.findByRole("dialog", { name: "Update links?" });
    expect(within(links).getByText("Home")).toBeTruthy();
    expect(within(links).getAllByText(/\[\[Bram\]\]/).length).toBeGreaterThan(0);
    const home = text(api, "Home.md");
    await user.click(within(links).getByRole("button", { name: "Rename only" }));
    await waitFor(() => expect(api.vault.files.has("People/Bram.md")).toBe(true));
    expect(text(api, "Home.md")).toBe(home);

    await user.click(screen.getByRole("button", { name: "Rename" }));
    const again = await screen.findByRole("dialog", { name: "Rename" });
    await user.clear(within(again).getByLabelText("New name"));
    await user.type(within(again).getByLabelText("New name"), "Bramblewick");
    await user.click(within(again).getByRole("button", { name: "Rename" }));
    // Nothing links to "Bram" now, so it renames at once.
    await waitFor(() => expect(api.vault.files.has("People/Bramblewick.md")).toBe(true));
    await user.click(screen.getByRole("button", { name: "Rename" }));
    const third = await screen.findByRole("dialog", { name: "Rename" });
    await user.clear(within(third).getByLabelText("New name"));
    await user.type(within(third).getByLabelText("New name"), "Bramblewick the Bold");
    await user.click(within(third).getByRole("button", { name: "Rename" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Update links?" })).getByRole("button", { name: "Rename and update links" }));
    await waitFor(() => expect(text(api, "Home.md")).toContain("[[Bramblewick the Bold#Appearance]]"));
  });

  it("deletes to the trash, and asks before deleting for good when there is none", async () => {
    const user = userEvent.setup();
    const api = await withVault();
    renderApp("/notes", { api });
    await openNote(user, "Odd", "Empty");
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Delete?" })).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(api.vault.trashed).toEqual(["Odd/Empty.md"]));

    api.vault.trashAvailable = false;
    await user.click(row("Board.canvas"));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Delete?" })).getByRole("button", { name: "Delete" }));
    const sure = await screen.findByRole("dialog", { name: "Delete for good?" });
    expect(api.vault.files.has("Board.canvas")).toBe(true);
    await user.click(within(sure).getByRole("button", { name: "Delete for good" }));
    await waitFor(() => expect(api.vault.files.has("Board.canvas")).toBe(false));
  });
});
