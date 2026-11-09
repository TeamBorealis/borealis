import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

// GitHub #37: an imported character that names content which is not installed said only how much was missing ("35 content
// element(s) were not found"), on the Build page and after Re-check missing content. Both now list WHICH elements: the
// names, types and ids the file records, grouped by the source the ids name, with the way to add that content.
const mock = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5 });
const file = (name: string) => new File(["<character/>"], name);
const b64 = (s: string) => btoa(s);

async function importMissing(api: MockApi): Promise<string> {
  const r = await api.importCharacter({ fileName: "needs-missing.dnd5e", contentBase64: b64("<character/>") });
  return r.summary.id;
}

describe("GitHub #37: the Build page lists the missing content its notice counts", () => {
  it("the notice opens the list: names, types, ids by source, and what to do", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const id = await importMissing(api);
    renderApp("/builder/race", { api, openCharacterId: id });
    const region = await screen.findByRole("region", { name: "Content notices" });
    expect(within(region).getByText(/3 content element\(s\) were not found/)).toBeTruthy();

    // Act
    await user.click(within(region).getByRole("button", { name: "Show missing content" }));

    // Assert
    const dialog = await screen.findByRole("dialog", { name: /Missing content/ });
    expect(within(dialog).getByText("3 content elements were not found.")).toBeTruthy();
    const list = within(dialog).getByRole("region", { name: "Missing content for needs-missing" });
    expect(within(list).getByRole("heading", { name: "HOMEBREW" })).toBeTruthy();
    expect(within(list).getByText("Radiant Blast")).toBeTruthy();
    expect(within(list).getByText("ID_HOMEBREW_SPELL_RADIANT_BLAST")).toBeTruthy();
    expect(within(list).getByText("Spell")).toBeTruthy();
    expect(within(dialog).getByText(/Add the content these come from in Additional Content/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Import the content it needs" })).toBeTruthy();
  });

  it("re-checks from the list: once the content is installed, nothing is missing and the notice goes", async () => {
    const user = userEvent.setup();
    const api = mock();
    const id = await importMissing(api);
    renderApp("/builder/race", { api, openCharacterId: id });
    const region = await screen.findByRole("region", { name: "Content notices" });
    await user.click(within(region).getByRole("button", { name: "Show missing content" }));
    const dialog = await screen.findByRole("dialog", { name: /Missing content/ });

    api.makeMissingContentAvailable();
    await user.click(within(dialog).getByRole("button", { name: "Re-check missing content" }));

    await waitFor(() => expect(within(dialog).getByText("Nothing is missing any more.")).toBeTruthy());
    expect(within(dialog).queryByRole("region", { name: /Missing content for/ })).toBeNull();
    expect(screen.queryByRole("region", { name: "Content notices" })).toBeNull();
  });

  it("a character with nothing missing has no Show missing content action", async () => {
    const api = mock();
    const r = await api.importCharacter({ fileName: "fine.dnd5e", contentBase64: b64("<character/>") });
    renderApp("/builder/race", { api, openCharacterId: r.summary.id });
    await screen.findByRole("tab", { name: "Class" });
    expect(screen.queryByRole("button", { name: "Show missing content" })).toBeNull();
  });
});

describe("GitHub #37: Re-check missing content on the Characters page lists what is still missing", () => {
  it("opens the list when content is still missing, and says so once it is found", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    renderApp("/characters", { api });
    await user.upload(await screen.findByLabelText("Import character files"), [file("needs-missing.dnd5e")]);
    await user.click(await screen.findByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Act
    await user.click(await screen.findByRole("button", { name: /Re-check missing content/ }));

    // Assert
    const dialog = await screen.findByRole("dialog", { name: /Missing content/ });
    const list = within(dialog).getByRole("region", { name: "Missing content for needs-missing" });
    expect(within(list).getByText("Radiant Blast")).toBeTruthy();
    expect(within(list).getByText("ID_HOMEBREW_SPELL_RADIANT_BLAST")).toBeTruthy();
    expect(within(dialog).getByText(/Add the content these come from in Additional Content/)).toBeTruthy();

    api.makeMissingContentAvailable();
    await user.click(within(dialog).getByRole("button", { name: "Re-check missing content" }));
    await waitFor(() => expect(within(dialog).getByText("Nothing is missing any more.")).toBeTruthy());
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("opens nothing when the re-check finds everything", async () => {
    const user = userEvent.setup();
    const api = mock();
    renderApp("/characters", { api });
    await user.upload(await screen.findByLabelText("Import character files"), [file("needs-missing.dnd5e")]);
    await user.click(await screen.findByRole("button", { name: "Done" }));
    api.makeMissingContentAvailable();
    await user.click(await screen.findByRole("button", { name: /Re-check missing content/ }));
    await waitFor(() => expect(screen.queryByRole("button", { name: /Re-check missing content/ })).toBeNull());
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
