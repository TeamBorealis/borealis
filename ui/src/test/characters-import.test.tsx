import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { groupMissing } from "../pages/characters/CharacterImportReportDialog";
import { renderApp } from "./utils";

const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5, ...extra });
const file = (name: string) => new File(["<character/>"], name);

describe("Characters page: import entry points", () => {
  it("shows an Import characters button on mobile, with a multi-select picker that accepts any file type", async () => {
    renderApp("/characters", { api: mock({ mobile: true }) });
    expect(await screen.findByRole("button", { name: /Import characters/ })).toBeTruthy();
    const input = screen.getByLabelText("Import character files");
    expect(input.hasAttribute("multiple")).toBe(true);
    expect(input.hasAttribute("accept")).toBe(false);
    expect(document.body.textContent).not.toMatch(/\bAI\b|Assistant|Codex|Claude|\bpair/i);
  });

  it("keeps the desktop start page without an extra button but accepts several files", async () => {
    renderApp("/characters", { api: mock() });
    await screen.findAllByRole("button", { name: /New character/ });
    expect(screen.queryByRole("button", { name: /Import characters/ })).toBeNull();
    const input = screen.getByLabelText("Import character files");
    expect(input.hasAttribute("multiple")).toBe(true);
    expect(input.getAttribute("accept")).toContain(".dnd5e");
  });
});

describe("Character import report", () => {
  it("reports every file of a batch, with missing content grouped by source and type", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mock() });
    const input = await screen.findByLabelText("Import character files");
    await user.upload(input, [file("fine.dnd5e"), file("needs-missing.dnd5e")]);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/2 of 2 characters imported, some with missing content/)).toBeTruthy();
    const missing = within(dialog).getByRole("region", { name: /Missing content for needs-missing/ });
    expect(within(missing).getByRole("heading", { name: "WOTC TCOE" })).toBeTruthy();
    expect(within(missing).getByRole("heading", { name: "HOMEBREW" })).toBeTruthy();
    expect(within(missing).getByText("Skill Expert")).toBeTruthy();
    expect(within(missing).getByText("Archetype Feature")).toBeTruthy();
    expect(within(dialog).getByRole("region", { name: /Unmapped selections for needs-missing/ })).toBeTruthy();
    expect(within(dialog).getAllByText(/legacy\.nonpositive_merge/).length).toBe(2);
    expect(within(dialog).getByRole("button", { name: "Import the content it needs" })).toBeTruthy();
  });

  it("continues after a failing file and says which one failed", async () => {
    const user = userEvent.setup();
    const api = mock();
    const real = api.importCharacter.bind(api);
    api.importCharacter = async (a) => { if (a.fileName.startsWith("broken")) throw new Error("not a character file"); return real(a); };
    renderApp("/characters", { api });
    await user.upload(await screen.findByLabelText("Import character files"), [file("broken.dnd5e"), file("fine.dnd5e")]);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/1 of 2 characters imported, 1 failed/)).toBeTruthy();
    expect(within(dialog).getByText(/Could not import this file: not a character file/)).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Import the content it needs" })).toBeNull();
  });

  it("offers no content actions for a clean import", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mock() });
    await user.upload(await screen.findByLabelText("Import character files"), [file("fine.dnd5e")]);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Imported.")).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Re-check missing content" })).toBeNull();
  });

  it("links to Additional Content", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mock() });
    await user.upload(await screen.findByLabelText("Import character files"), [file("needs-missing.dnd5e")]);
    await user.click(await screen.findByRole("button", { name: "Import the content it needs" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText("Installed packs")).toBeTruthy();
  });

  it("re-checks missing content: items disappear once the content is installed, with no re-import", async () => {
    const user = userEvent.setup();
    const api = mock();
    renderApp("/characters", { api });
    await user.upload(await screen.findByLabelText("Import character files"), [file("needs-missing.dnd5e")]);
    let dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Re-check missing content" }));
    await waitFor(() => expect(within(dialog).getByRole("region", { name: /Missing content for/ })).toBeTruthy());

    api.makeMissingContentAvailable();
    await user.click(within(dialog).getByRole("button", { name: "Re-check missing content" }));
    await waitFor(() => expect(within(dialog).queryByRole("region", { name: /Missing content for/ })).toBeNull());
    dialog = screen.getByRole("dialog");
    expect(within(dialog).queryByRole("button", { name: "Re-check missing content" })).toBeNull();
    expect((await api.listCharacters()).every((c) => c.missingContent === 0)).toBe(true);
  });

  it("shows a Re-check missing content action on the page while characters wait for content", async () => {
    const user = userEvent.setup();
    const api = mock();
    renderApp("/characters", { api });
    expect(screen.queryByRole("button", { name: /Re-check missing content/ })).toBeNull();
    await user.upload(await screen.findByLabelText("Import character files"), [file("needs-missing.dnd5e")]);
    await user.click(await screen.findByRole("button", { name: "Done" }));
    const page = await screen.findByRole("button", { name: /Re-check missing content/ });
    api.makeMissingContentAvailable();
    await user.click(page);
    await waitFor(() => expect(screen.queryByRole("button", { name: /Re-check missing content/ })).toBeNull());
  });
});

describe("groupMissing", () => {
  it("groups by source then type, with unknowns last-resort labelled", () => {
    const g = groupMissing([
      { id: "a", type: "Feat", name: "A", context: "selection", source: "PHB" },
      { id: "b", type: "", name: "B", context: "selection", source: "" },
      { id: "c", type: "Spell", name: "C", context: "selection", source: "PHB" },
    ]);
    expect(g.map(([s]) => s)).toEqual(["PHB", "Unknown source"]);
    expect(g[0]?.[1].map(([t]) => t)).toEqual(["Feat", "Spell"]);
    expect(g[1]?.[1][0]?.[0]).toBe("Other");
  });
});
