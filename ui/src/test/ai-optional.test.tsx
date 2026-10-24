import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { DEFAULT_SETTINGS } from "../api/mock/default-settings";
import { openBorealisSettings, renderApp } from "./utils";

const AI_TEXT = /\bAI\b|Assistant|Codex|Claude|\bpair/i;
const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5, ...extra });

async function openMenu(user: ReturnType<typeof userEvent.setup>, name: string): Promise<string[]> {
  await user.click(await screen.findByRole("menuitem", { name }));
  const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent ?? "");
  await user.keyboard("{Escape}");
  return items;
}

function drop(files: File[]) {
  fireEvent.drop(window, { dataTransfer: { types: ["Files"], files } });
}

describe("AI is optional on desktop", () => {
  it("is off by default: no AI text, entries or redirects outside Settings", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mock({ aiSetup: { completed: false } }) });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(AI_TEXT);
    expect((await openMenu(user, "View")).join("|")).not.toMatch(AI_TEXT);
    expect((await openMenu(user, "Help")).join("|")).not.toMatch(AI_TEXT);
  });

  it("the Settings switch reveals the provider controls and every AI entry", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await openBorealisSettings(user, "AI assistant");
    const toggle = screen.getByRole("checkbox", { name: "Enable AI assistant (optional)" }) as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    expect(screen.getByText("Uses your own Claude or Codex command-line tool.")).toBeTruthy();
    expect(screen.queryByLabelText("AI PROVIDER")).toBeNull();
    expect(screen.queryByRole("button", { name: "AI setup…" })).toBeNull();
    await user.click(toggle);
    expect(await screen.findByLabelText("AI PROVIDER")).toBeTruthy();
    expect(screen.getByRole("button", { name: "AI setup…" })).toBeTruthy();
    expect((await api.getSettings()).ai.enabled).toBe(true);
    await user.click(within(screen.getByRole("dialog", { name: "Borealis Settings" })).getAllByRole("button", { name: "Close" }).at(-1)!);
    expect(await openMenu(user, "View")).toContain("AI Assistant");
    expect(await openMenu(user, "Help")).toContain("AI Setup");
    expect(screen.getByRole("link", { name: "Import content" }).getAttribute("href")).toBe("/import");
  });

  it.each(["/ai", "/setup/ai", "/import"])("%s redirects to the characters while AI is off", async (route) => {
    renderApp(route);
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
  });

  it("a settings file written before the switch existed reads as off", async () => {
    const api = mock();
    const legacy = { ...DEFAULT_SETTINGS, ai: { provider: "claude", pairedHost: null, overridePath: null } } as unknown as typeof DEFAULT_SETTINGS;
    Object.assign(api, { getSettings: async () => legacy });
    renderApp("/ai", { api });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(AI_TEXT);
  });
});

describe("No AI on mobile", () => {
  it("shows no AI text anywhere, even if a settings file says AI is on", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mock({ mobile: true, aiEnabled: true }) });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(AI_TEXT);
    expect((await openMenu(user, "View")).join("|")).not.toMatch(AI_TEXT);
    expect((await openMenu(user, "Help")).join("|")).not.toMatch(AI_TEXT);
  });

  it("has no AI or Phones tab in Borealis Settings and cannot enable AI", async () => {
    const user = userEvent.setup();
    const api = mock({ mobile: true });
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Borealis Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Borealis Settings" });
    expect(within(dialog).getAllByRole("tab").map((t) => t.textContent)).toEqual(["Devices", "Content", "General"]);
    expect(dialog.textContent).not.toMatch(AI_TEXT);
    await api.saveSettings({ patch: { ai: { ...DEFAULT_SETTINGS.ai, enabled: true } } });
    expect((await api.getSettings()).ai.enabled).toBe(false);
  });

  it.each(["/ai", "/setup/ai", "/import"])("%s redirects to the characters", async (route) => {
    renderApp(route, { api: mock({ mobile: true, aiEnabled: true }) });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
  });

  it("lists no AI destination under More", async () => {
    renderApp("/more", { api: mock({ mobile: true }) });
    const list = await screen.findByRole("list", { name: "More destinations" });
    expect(list.textContent).not.toMatch(AI_TEXT);
    expect(within(list).queryByText("Import")).toBeNull();
    expect(within(list).getByText("Compendium")).toBeTruthy();
  });
});

describe("Import without AI", () => {
  const file = (name: string, text = "<x/>", type = "") => new File([text], name, { type });

  it("converts XML and .index together into one preview, without installing", async () => {
    const api = mock();
    const previewImportPack = vi.fn(async (_a: { request: { files?: { fileName: string }[] } }) => ({
      previewId: "p1", packName: "Homebrew", packId: "homebrew", fileCount: 2, converted: 4, skipped: 1, counts: [{ type: "Feat", count: 4 }],
      summary: { errors: 0, warnings: 0, infos: 0 }, diagnostics: [], diagnosticsTruncated: false, unresolved: [], unresolvedTotal: 0, replacesExisting: false,
    }));
    const importPack = vi.fn();
    Object.assign(api, { previewImportPack, importPack });
    renderApp("/characters", { api });
    await screen.findByRole("heading", { name: "Characters" });
    drop([file("a.xml"), file("core.index")]);
    await waitFor(() => expect(previewImportPack).toHaveBeenCalledTimes(1));
    expect(previewImportPack.mock.calls[0]![0].request.files!.map((f) => f.fileName)).toEqual(["a.xml", "core.index"]);
    expect(await screen.findByRole("button", { name: "Install" })).toBeTruthy();
    expect(importPack).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "Import" })).toBeNull();
  });

  it("sends .dnd5e to the character importer and .json by its content", async () => {
    const api = mock();
    const importCharacter = vi.fn(async () => ({ summary: { name: "Aldric" }, divergences: [], warnings: [] }));
    const previewImportPack = vi.fn(async () => ({
      previewId: "p2", packName: "p", packId: "p", fileCount: 1, converted: 1, skipped: 0, counts: [], summary: { errors: 0, warnings: 0, infos: 0 },
      diagnostics: [], diagnosticsTruncated: false, unresolved: [], unresolvedTotal: 0, replacesExisting: false,
    }));
    Object.assign(api, { importCharacter, previewImportPack });
    renderApp("/characters", { api });
    await screen.findByRole("heading", { name: "Characters" });
    drop([file("hero.dnd5e"), file("hero.json", '{"id":"c1","name":"Aldric"}'), file("p.json", '{"format":"borealis-content/1","elements":[]}')]);
    await waitFor(() => expect(importCharacter).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(previewImportPack).toHaveBeenCalledTimes(1));
  });

  it("explains that PDFs and photos need the optional AI assistant, without opening the AI wizard", async () => {
    renderApp("/characters");
    await screen.findByRole("heading", { name: "Characters" });
    drop([file("sheet.pdf", "%PDF", "application/pdf")]);
    expect(await screen.findByText(/sheet\.pdf cannot be imported directly\. PDFs, photos and plain text need the optional AI assistant/)).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Import" })).toBeNull();
  });

  it("uses a file picker for the + button", async () => {
    renderApp("/characters");
    const fab = await screen.findByRole("button", { name: "Import content" });
    expect(fab).toBeTruthy();
    expect(screen.getByLabelText("Choose files to import").getAttribute("accept")).toContain(".dnd5e");
  });

  it("gives a neutral message with no AI text on mobile", async () => {
    renderApp("/characters", { api: mock({ mobile: true }) });
    await screen.findByRole("heading", { name: "Characters" });
    drop([file("scan.png", "x", "image/png")]);
    const msg = await screen.findByText(/scan\.png cannot be imported here/, {}, { timeout: 5000 });
    expect(msg.textContent).not.toMatch(AI_TEXT);
  });

  it("with AI on, other files still go to the AI import page", async () => {
    renderApp("/characters", { aiEnabled: true });
    await screen.findByRole("heading", { name: "Characters" });
    drop([file("notes.txt", "a class", "text/plain")]);
    expect(await screen.findByRole("heading", { name: "Import" })).toBeTruthy();
  });
});
