import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { regionPaper, sheetPaper } from "../lib/paper";
import { renderApp, seededId } from "./utils";

// B-147 (tester eylonke, 2026-11-02; decision TV1): a Paper setting, Letter or A4, defaulting by the device region
// (Letter for US, CA, MX, PH and the other Letter regions, else A4). A4 output adds "· A4" to the page footer.

const languages = (tags: string[]) => vi.spyOn(navigator, "languages", "get").mockReturnValue(tags);

afterEach(() => { vi.restoreAllMocks(); });

describe("B-147 paper default by region", () => {
  it.each([
    [["en-US"], "letter"], [["en-CA"], "letter"], [["fr-CA"], "letter"], [["es-MX"], "letter"], [["en-PH"], "letter"],
    [["es-CL"], "letter"], [["he-IL"], "a4"], [["en-GB"], "a4"], [["de-DE"], "a4"], [["en-AU"], "a4"], [["en-IN"], "a4"],
  ] as const)("%j prints on %s", (tags, paper) => {
    expect(regionPaper(tags)).toBe(paper);
  });

  it("uses a language's likely region when the tag names none, and A4 when nothing tells", () => {
    expect(regionPaper(["he"])).toBe("a4");
    expect(regionPaper(["en"])).toBe("letter");
    expect(regionPaper([])).toBe("a4");
    expect(regionPaper(["not a tag", "en-US"])).toBe("letter");
  });

  it("a chosen paper wins over the region", () => {
    expect(sheetPaper("letter", ["he-IL"])).toBe("letter");
    expect(sheetPaper("a4", ["en-US"])).toBe("a4");
    expect(sheetPaper(null, ["en-US"])).toBe("letter");
  });
});

async function openSheetSettings(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
  await user.click(within(dialog).getByRole("tab", { name: "Character sheet" }));
  return dialog;
}

describe("B-147 Paper setting", () => {
  it("offers Letter and A4, shows the region's paper until one is picked, and saves the pick", async () => {
    languages(["he-IL"]);
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const dialog = await openSheetSettings(user);
    const paper = within(dialog).getByLabelText("PAPER") as HTMLSelectElement;
    expect([...paper.options].map((o) => o.textContent)).toEqual(["Letter", "A4"]);
    expect(paper.value).toBe("a4");
    expect((await api.getSettings()).sheet.paper).toBeNull();
    await user.selectOptions(paper, "letter");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).sheet.paper).toBe("letter");
  });

  it("defaults to Letter on a US device", async () => {
    languages(["en-US"]);
    const user = userEvent.setup();
    renderApp("/characters");
    const dialog = await openSheetSettings(user);
    expect((within(dialog).getByLabelText("PAPER") as HTMLSelectElement).value).toBe("letter");
  });
});

describe("B-147 preview and export follow the paper", () => {
  const open = async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    return { api, id };
  };

  it("an A4 region's preview has A4 pages with '· A4' in the footer, and the export is asked for A4", async () => {
    languages(["en-GB"]);
    const { api, id } = await open();
    const render = vi.spyOn(api, "renderSheet");
    const exportSheet = vi.spyOn(api, "exportSheet");
    const user = userEvent.setup();
    renderApp("/sheet", { api, openCharacterId: id });
    const frame = (await screen.findByTitle("Character sheet preview")) as HTMLIFrameElement;
    expect(frame.srcdoc).toContain('class="page a4"');
    expect(frame.srcdoc).toContain("· A4</");
    expect(render).toHaveBeenCalledWith({ characterId: id, options: { defaultPaper: "a4" } });
    await user.click(screen.getByRole("button", { name: "Save Character Sheet" }));
    await waitFor(() => expect(exportSheet).toHaveBeenCalledWith({ characterId: id, options: { format: "pdf", defaultPaper: "a4" } }));
  });

  it("a chosen Letter prints Letter pages without the A4 mark, whatever the region", async () => {
    languages(["en-GB"]);
    const { api, id } = await open();
    await api.saveSettings({ patch: { sheet: { ...(await api.getSettings()).sheet, paper: "letter" } } });
    renderApp("/sheet", { api, openCharacterId: id });
    const frame = (await screen.findByTitle("Character sheet preview")) as HTMLIFrameElement;
    expect(frame.srcdoc).toContain('class="page letter"');
    expect(frame.srcdoc).not.toContain("· A4</");
  });
});
