import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { buildLine } from "../pages/characters/CharacterTile";
import { renderApp } from "./utils";

describe("character tiles", () => {
  it("a single click selects and shows corner edit/delete; nothing is loaded", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const tile = await screen.findByRole("group", { name: "Seraphine Vale" });
    expect(within(tile).queryByRole("button", { name: /^Edit / })).toBeNull();
    await user.click(within(tile).getByRole("button", { name: /Seraphine Vale, Level 2/ }));
    expect(within(tile).getByRole("button", { name: "Edit Seraphine Vale" })).toBeTruthy();
    expect(within(tile).getByRole("button", { name: "Delete Seraphine Vale" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Seraphine Vale" })).toBeNull(); // the gold character menu only exists once loaded
    expect(screen.queryByText(/^Loaded 'Seraphine Vale'/)).toBeNull();
  });

  it("a double click loads the character and goes to Build", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const tile = await screen.findByRole("group", { name: "Seraphine Vale" });
    await user.dblClick(within(tile).getByRole("button", { name: /Seraphine Vale, Level 2/ }));
    expect(await screen.findByText("Loaded 'Seraphine Vale'")).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("group", { name: "Seraphine Vale" })).toBeNull());
  });

  it("Enter on a tile loads it", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const tile = await screen.findByRole("group", { name: "Brannoc Ironvale" });
    within(tile).getByRole("button", { name: /Brannoc Ironvale/ }).focus();
    await user.keyboard("{Enter}");
    expect(await screen.findByText("Loaded 'Brannoc Ironvale'")).toBeTruthy();
  });

  it("has no LOAD or search controls on the page; the build line reads LEVEL n RACE CLASS", async () => {
    renderApp("/characters");
    await screen.findByRole("group", { name: "Seraphine Vale" });
    expect(screen.queryByRole("button", { name: /load character/i })).toBeNull();
    expect(screen.queryByRole("searchbox", { name: "Search characters" })).toBeNull();
    expect(buildLine({ build: "Mountain Dwarf Fighter 1", level: 1 })).toBe("Level 1 Mountain Dwarf Fighter");
  });

  it("File > Search Characters reveals the search box", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await screen.findByRole("group", { name: "Seraphine Vale" });
    await user.click(screen.getByRole("menuitem", { name: "File" }));
    await user.click(await screen.findByRole("menuitem", { name: /Search Characters/ }));
    const box = await screen.findByRole("searchbox", { name: "Search characters" });
    await user.type(box, "Bran");
    await waitFor(() => expect(screen.queryByRole("group", { name: "Seraphine Vale" })).toBeNull());
    expect(screen.getByRole("group", { name: "Brannoc Ironvale" })).toBeTruthy();
  });
});

describe("Additional content and Sources", () => {
  it("stacks the sections with no nested tab row", async () => {
    renderApp("/content");
    await screen.findByRole("region", { name: "Index files" });
    expect(screen.getByRole("region", { name: "Custom content" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "5etools import" })).toBeTruthy();
    expect(screen.queryByRole("tablist", { name: "Content sections" })).toBeNull();
  });

  it("an index that is not signed shows the unverified badge", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    await api.addIndex({ url: "https://example.com/sample.index" });
    await user.click(await screen.findByRole("button", { name: "Update" }));
    expect(await screen.findByText("unverified")).toBeTruthy();
  });

  it("Sources is a DataGrid with Name, Source group, Author and Version", async () => {
    renderApp("/content/sources");
    const grid = await screen.findByRole("grid", { name: "Sources" });
    for (const h of ["Name", "Source group", "Author", "Version"]) expect(within(grid).getByRole("columnheader", { name: h })).toBeTruthy();
    expect((await within(grid).findAllByRole("row")).length).toBeGreaterThan(1);
  });
});

describe("Compendium flyout", () => {
  it("Enter in the title-bar search box opens the bottom flyout with the query; Escape closes it", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.type(await screen.findByLabelText("Compendium search"), "fireball{Enter}");
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });
    expect((within(dlg).getByLabelText("Search the compendium") as HTMLInputElement).value).toBe("fireball");
    expect(await within(dlg).findByRole("button", { name: /Fireball/ })).toBeTruthy();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
  });

  it("F1 opens it and Alt+Q focuses the search box", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const box = await screen.findByLabelText("Compendium search");
    await user.keyboard("{Alt>}q{/Alt}");
    expect(document.activeElement).toBe(box);
    await user.keyboard("{F1}");
    expect(await screen.findByRole("dialog", { name: "Compendium" })).toBeTruthy();
  });

  it("View > Compendium opens it", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: /Compendium/ }));
    expect(await screen.findByRole("dialog", { name: "Compendium" })).toBeTruthy();
  });
});
