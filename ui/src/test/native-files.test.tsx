import { act, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { closeImportPreview } from "../state/importPreview";
import { renderApp } from "./utils";

const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });
const PACK = `<elements><element name="Lantern" type="Feat" source="T" id="ID_LANTERN"><rules/></element></elements>`;

async function openContent(api: MockApi) {
  renderApp("/content", { api });
  await screen.findByRole("button", { name: "Import files" });
}

describe("Native file drop (desktop app): tokens only, read through takePickedFiles", () => {
  beforeEach(() => closeImportPreview());

  it("previews a dropped XML pack after reading it with its dropped-file token", async () => {
    const api = mock();
    const take = vi.spyOn(api, "takePickedFiles");
    await openContent(api);
    act(() => api.dropFiles([{ name: "lantern.xml", contentBase64: btoa(PACK) }]));
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/1 element from 1 file will be added as "lantern"/)).toBeTruthy();
    expect(take).toHaveBeenCalledTimes(1);
    const [args] = take.mock.calls[0]!;
    expect(args.purpose).toBe("dropped-file");
    expect(args.tokens).toHaveLength(1);
    expect(args.tokens[0]).toMatch(/^pick_/);
  });

  it("previews a dropped folder like the Import my content folder picker", async () => {
    const api = mock();
    const preview = vi.spyOn(api, "previewImportPack");
    api.folderFiles = [{ fileName: "lantern.xml", contentBase64: btoa(PACK) }];
    await openContent(api);
    act(() => api.dropFolder("My Content"));
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/1 element from 1 file will be added as "My Content"/)).toBeTruthy();
    const [args] = preview.mock.calls[0]!;
    expect(args.request.folder).toMatch(/^pick_/);
    expect(args.request.files).toBeUndefined();
  });

  it("imports a dropped character file and opens the character list", async () => {
    const api = mock();
    const importCharacter = vi.spyOn(api, "importCharacter");
    await openContent(api);
    act(() => api.dropFiles([{ name: "brannoc.dnd5e", contentBase64: btoa(JSON.stringify({ name: "Brannoc Ironvale" })) }]));
    expect(await screen.findByText("Imported Brannoc Ironvale.")).toBeTruthy();
    expect(importCharacter).toHaveBeenCalledWith(expect.objectContaining({ fileName: "brannoc.dnd5e" }));
    expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull();
  });

  it("names dropped files it cannot import and never reads them", async () => {
    const api = mock();
    const take = vi.spyOn(api, "takePickedFiles");
    await openContent(api);
    act(() => api.dropFiles([{ name: "id_rsa", contentBase64: btoa("secret") }]));
    expect(await screen.findByText(/id_rsa/)).toBeTruthy();
    expect(take).not.toHaveBeenCalled();
  });

  it("refuses a dropped token for any other purpose, and a second read", async () => {
    const api = mock();
    let tokens: string[] = [];
    api.subscribe("files:dropped", (d) => { tokens = d.files.map((f) => f.token); });
    api.takePickedFiles = vi.fn(api.takePickedFiles.bind(api)).mockImplementationOnce(async () => []); // keep the UI from consuming it
    await openContent(api);
    act(() => api.dropFiles([{ name: "a.xml", contentBase64: btoa(PACK) }]));
    await waitFor(() => expect(tokens).toHaveLength(1));
    await expect(api.takePickedFiles({ purpose: "character-file", tokens })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.takePickedFiles({ purpose: "dropped-file", tokens })).resolves.toHaveLength(1);
    await expect(api.takePickedFiles({ purpose: "dropped-file", tokens })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.pickPath({ purpose: "dropped-file" })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});

describe("File > Load Character (desktop)", () => {
  it("opens the backend's native panel instead of clicking a file input", async () => {
    const api = mock();
    const pick = vi.spyOn(api, "pickPath");
    const take = vi.spyOn(api, "takePickedFiles");
    api.picks.contents.set("/home/me/Documents/Mira.dnd5e", btoa(JSON.stringify({ name: "Mira Vale" })));
    const click = vi.spyOn(HTMLInputElement.prototype, "click");
    renderApp("/characters?load=1", { api });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Imported.")).toBeTruthy();
    expect(pick).toHaveBeenCalledWith({ purpose: "character-file" });
    expect(take).toHaveBeenCalledWith(expect.objectContaining({ purpose: "character-file" }));
    expect(click).not.toHaveBeenCalled();
    click.mockRestore();
  });

  it("does nothing when the panel is cancelled", async () => {
    const api = mock();
    api.picks.next = null;
    const take = vi.spyOn(api, "takePickedFiles");
    renderApp("/characters?load=1", { api });
    await screen.findAllByRole("button", { name: /New character/ });
    await new Promise((r) => setTimeout(r, 20));
    expect(take).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps the system picker on mobile", async () => {
    const api = mock({ mobile: true });
    const pick = vi.spyOn(api, "pickPath");
    const click = vi.spyOn(HTMLInputElement.prototype, "click").mockImplementation(() => {});
    renderApp("/characters?load=1", { api });
    await screen.findByRole("button", { name: /Import characters/ });
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect(pick).not.toHaveBeenCalled();
    click.mockRestore();
  });
});
