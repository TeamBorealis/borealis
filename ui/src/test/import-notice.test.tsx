import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { closeImportPreview } from "../state/importPreview";
import { renderApp } from "./utils";

const fast = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, ...extra });
const NOTICE = { name: "Import only content you own" };
const noticeDialog = () => screen.findByRole("dialog", NOTICE);
const xml = `<elements><element name="Alpha" type="Feat" source="T" id="ID_A"><rules/></element></elements>`;

/** Count clicks on the hidden file input; a real picker would open on each. */
function spyPicker(label: string) {
  const input = screen.getByLabelText(label) as HTMLInputElement;
  return vi.spyOn(input, "click");
}

describe("Content import notice", () => {
  beforeEach(() => closeImportPreview());

  it("Import files: the notice comes first, Cancel opens no picker, continue does", async () => {
    const user = userEvent.setup();
    renderApp("/content", { api: fast(), importNotice: true });
    const click = spyPicker("Choose content files");
    await user.click(await screen.findByRole("button", { name: "Import files" }));
    const dialog = await noticeDialog();
    expect(within(dialog).getByText(/Borealis does not provide content, does not condone importing material in breach of copyright/)).toBeTruthy();
    expect(click).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", NOTICE)).toBeNull();
    expect(click).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Import files" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "I understand, continue" }));
    await waitFor(() => expect(click).toHaveBeenCalledTimes(1));
  });

  it("Import my content folder: no native picker until the notice is accepted", async () => {
    const user = userEvent.setup();
    const api = fast();
    const pick = vi.spyOn(api, "pickPath");
    renderApp("/content", { api, importNotice: true });
    await user.click(await screen.findByRole("button", { name: "Import my content folder" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(pick).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Import my content folder" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "I understand, continue" }));
    await waitFor(() => expect(pick).toHaveBeenCalledWith(expect.objectContaining({ purpose: "content-folder" })));
  });

  it("5etools Scan folder: Cancel makes no picker call, continue does", async () => {
    const user = userEvent.setup();
    const api = fast();
    const pick = vi.spyOn(api, "pickPath");
    renderApp("/content", { api, importNotice: true });
    await user.click(await screen.findByRole("button", { name: "Scan folder…" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(pick).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Scan folder…" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "I understand, continue" }));
    expect(await screen.findByRole("list", { name: "Books sources" })).toBeTruthy();
    expect(pick).toHaveBeenCalledTimes(1);
  });

  it("an OS-opened content file waits for the notice; Cancel reads and previews nothing", async () => {
    const user = userEvent.setup();
    const api = fast();
    const preview = vi.spyOn(api, "previewImportPack");
    api.openedFiles = [{ fileName: "shared.xml", contentBase64: btoa(xml) }];
    renderApp("/characters", { api, importNotice: true });
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(preview).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull();
  });

  it("a character file is not gated", async () => {
    const api = fast();
    const importCharacter = vi.spyOn(api, "importCharacter");
    api.openedFiles = [{ fileName: "hero.dnd5e", contentBase64: btoa("<x/>") }];
    renderApp("/characters", { api, importNotice: true });
    await waitFor(() => expect(importCharacter).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("dialog", NOTICE)).toBeNull();
  });

  it("AI import: Choose files and Browse are gated, and so is sending", async () => {
    const user = userEvent.setup();
    const api = fast({ aiEnabled: true });
    renderApp("/import", { api, importNotice: true });
    const choose = await screen.findByRole("button", { name: "Choose files" });
    const click = spyPicker("Choose files");
    await user.click(choose);
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(click).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Choose files" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "I understand, continue" }));
    await waitFor(() => expect(click).toHaveBeenCalledTimes(1));
    const start = vi.spyOn(api, "importStart");
    await user.type(screen.getByLabelText("Paste text"), "A feat that does a thing");
    await user.click(screen.getByRole("button", { name: "Import" }));
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(start).not.toHaveBeenCalled();
  });

  it("Don't show this again persists, skips the next import, and Settings brings it back", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/content", { api, importNotice: true });
    await user.click(await screen.findByRole("button", { name: "Import my content folder" }));
    const dialog = await noticeDialog();
    await user.click(within(dialog).getByRole("checkbox", { name: "Don't show this again" }));
    await user.click(within(dialog).getByRole("button", { name: "I understand, continue" }));
    await waitFor(async () => expect((await api.getSettings()).showImportNotice).toBe(false));
    const pick = vi.spyOn(api, "pickPath");
    await user.click(screen.getByRole("button", { name: "Import my content folder" }));
    await waitFor(() => expect(pick).toHaveBeenCalled());
    expect(screen.queryByRole("dialog", NOTICE)).toBeNull();

    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    const settings = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(await within(settings).findByRole("tab", { name: "Data files" }));
    await user.click(within(settings).getByRole("checkbox", { name: "Content Import Notice" }));
    await user.click(within(settings).getByRole("button", { name: "Save" }));
    await waitFor(async () => expect((await api.getSettings()).showImportNotice).toBe(true));
    await user.click(screen.getByRole("button", { name: "Import my content folder" }));
    expect(await noticeDialog()).toBeTruthy();
  });

  it("the Content page can re-read the notice without side effects", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/content", { api, importNotice: true });
    await user.click(await screen.findByRole("button", { name: "Content import notice" }));
    const dialog = await noticeDialog();
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    await user.click(within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!);
    expect(screen.queryByRole("dialog", NOTICE)).toBeNull();
    expect((await api.getSettings()).showImportNotice).toBe(true);
  });

  it("file input path (drop-in) is gated when it bypasses the button", async () => {
    const user = userEvent.setup();
    const api = fast();
    const preview = vi.spyOn(api, "previewImportPack");
    renderApp("/content", { api, importNotice: true });
    await screen.findByRole("button", { name: "Import files" });
    fireEvent.change(screen.getByLabelText("Choose content files"), { target: { files: [new File([xml], "a.xml")] } });
    await user.click(within(await noticeDialog()).getByRole("button", { name: "Cancel" }));
    expect(preview).not.toHaveBeenCalled();
  });
});
