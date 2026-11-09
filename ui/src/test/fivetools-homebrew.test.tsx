import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { classifyImport, looksLikeFiveTools } from "../lib/importRouting";
import { sourceDescription } from "../pages/content/fiveTools";
import { HOMEBREW_BUTTON } from "../pages/content/FiveToolsTab";
import { renderApp } from "./utils";

/** B-238: single 5etools-format homebrew files (invented content). */
const BREW = JSON.stringify({
  _meta: { sources: [{ json: "HBX", full: "My Homebrew" }], dependencies: { spell: ["OtherBrew"] } },
  feat: [{ name: "Steady Hand", source: "HBX", entries: ["Steady."] }],
  deity: [{ name: "The First Scribe", source: "HBX" }],
});
const brewFile = (name = "my-brew.json", text = BREW) => new File([text], name, { type: "application/json" });
const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });
const chooseBrew = (files: File[]) => fireEvent.change(screen.getByLabelText("Choose 5etools-format homebrew files"), { target: { files } });

describe("recognising 5etools-format homebrew", () => {
  it("tells a homebrew file from a Borealis pack and a character", async () => {
    expect(looksLikeFiveTools(BREW)).toBe(true);
    expect(looksLikeFiveTools(JSON.stringify({ spell: [{ name: "A" }] }))).toBe(true);
    expect(looksLikeFiveTools(JSON.stringify({ _meta: {}, elements: [] }))).toBe(false);
    expect(looksLikeFiveTools(JSON.stringify({ name: "Hero", level: 3 }))).toBe(false);
    expect(looksLikeFiveTools("{ broken")).toBe(false);
    expect(await classifyImport(brewFile())).toBe("fivetools");
    expect(await classifyImport(brewFile("pack.json", JSON.stringify({ format: "borealis-content/1", elements: [] })))).toBe("pack");
    expect(await classifyImport(brewFile("hero.json", JSON.stringify({ name: "Hero" })))).toBe("character");
  });

  it("describes what a source holds, what is not imported and what it needs", () => {
    expect(sourceDescription({ abbr: "HBX", name: "H", ruleset: "dnd2014", category: "homebrew", counts: { feat: 2 }, unsupported: { deity: 1 }, needs: ["OtherBrew"] }))
      .toBe("2 feat · not imported: 1 deity · needs OtherBrew (not chosen; choose its file too)");
    expect(sourceDescription({ abbr: "B", name: "B", ruleset: "dnd2014", category: "book", counts: { feat: 1 }, unsupported: {}, needs: [] })).toBe("1 feat");
  });
});

describe("5etools-format import: single homebrew files", () => {
  it("on phones offers the homebrew chooser (several files, untyped files allowed) and scans the files", async () => {
    const api = mock({ mobile: true });
    const stage = vi.spyOn(api, "stage5etoolsFiles");
    renderApp("/content", { api });
    await screen.findByRole("button", { name: HOMEBREW_BUTTON });
    await screen.findByLabelText("Data folder .zip");
    const input = screen.getByLabelText("Choose 5etools-format homebrew files") as HTMLInputElement;
    expect(input.multiple).toBe(true);
    expect(input.accept).toContain(".json");
    expect(input.accept).toContain("application/octet-stream");

    chooseBrew([brewFile(), brewFile("more.json")]);
    const list = await screen.findByRole("list", { name: "Homebrew & other sources" });
    expect(stage).toHaveBeenCalledTimes(1);
    expect(stage.mock.calls[0]![0].files.map((f) => f.fileName)).toEqual(["my-brew.json", "more.json"]);
    expect((screen.getByLabelText("Data folder .zip") as HTMLInputElement).value).toBe("my-brew.json and 1 more");
    expect(within(list).getByText(/not imported: 1 deity/)).toBeTruthy();
    expect(within(list).getByText(/needs OtherBrew/)).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("button", { name: /Import \d+ book\(s\)/ }));
    expect(await screen.findByRole("heading", { name: "Import report" })).toBeTruthy();
  });

  it("desktop has the homebrew chooser next to the folder scan, without untyped files", async () => {
    renderApp("/content", { api: mock() });
    expect(await screen.findByRole("button", { name: "Scan folder…" })).toBeTruthy();
    expect(screen.getByRole("button", { name: HOMEBREW_BUTTON })).toBeTruthy();
    expect((screen.getByLabelText("Choose 5etools-format homebrew files") as HTMLInputElement).accept).not.toContain("octet-stream");
  });

  it("shows the backend's refusal for a Borealis content file", async () => {
    renderApp("/content", { api: mock() });
    await screen.findByRole("button", { name: HOMEBREW_BUTTON });
    chooseBrew([brewFile("pack.json", JSON.stringify({ format: "borealis-content/1", elements: [] }))]);
    await waitFor(() => expect(screen.getByText(/is a Borealis content file/)).toBeTruthy());
    expect(screen.queryByRole("list", { name: "Homebrew & other sources" })).toBeNull();
  });

  it("the + button sends a homebrew file to the 5etools-format import, not the character import", async () => {
    const api = mock();
    const importCharacter = vi.spyOn(api, "importCharacter");
    const stage = vi.spyOn(api, "stage5etoolsFiles");
    renderApp("/characters", { api });
    await screen.findByRole("heading", { name: "Characters" });
    fireEvent.change(screen.getByLabelText("Choose files to import"), { target: { files: [brewFile()] } });
    expect(await screen.findByRole("list", { name: "Homebrew & other sources" })).toBeTruthy();
    expect(stage).toHaveBeenCalledTimes(1);
    expect(importCharacter).not.toHaveBeenCalled();
  });

  it("Custom content sends a homebrew file to the 5etools-format import instead of failing", async () => {
    const api = mock();
    const preview = vi.spyOn(api, "previewImportPack");
    const stage = vi.spyOn(api, "stage5etoolsFiles");
    renderApp("/content", { api });
    await screen.findByRole("button", { name: HOMEBREW_BUTTON });
    fireEvent.change(screen.getByLabelText("Choose content files"), { target: { files: [brewFile()] } });
    expect(await screen.findByText(/my-brew.json is a 5etools-format homebrew file/)).toBeTruthy();
    expect(await screen.findByRole("list", { name: "Homebrew & other sources" })).toBeTruthy();
    expect(stage).toHaveBeenCalledTimes(1);
    expect(preview).not.toHaveBeenCalled();
  });
});
