import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { closeImportPreview } from "../state/importPreview";
import { renderApp } from "./utils";

// A lone .index opened from outside the app is offered as a content source (same outcome as Indexes > Fetch).
const SOURCE = "https://example.invalid/hb/master/hb.index";
const index = `<index><info><name>Hb</name><update version="1.0"><file name="hb.index" url="${SOURCE}"/></update></info><files><file name="hb.xml" url="https://example.invalid/hb/master/hb.xml"/></files></index>`;
const feat = `<elements><element name="Alpha" type="Feat" source="T" id="ID_A"><rules/></element></elements>`;
const opened = (fileName: string, text: string) => ({ fileName, contentBase64: btoa(text) });

/** Hand files to the app the way the OS does and wait for the import preview (past the first-import notice if shown). */
async function openWith(api: MockApi, files: ReturnType<typeof opened>[]) {
  renderApp("/characters", { api });
  await screen.findByRole("button", { name: /Import characters/ });
  api.openedFiles = files;
  act(() => { window.dispatchEvent(new CustomEvent("borealis-open-file")); });
  return screen.findByRole("dialog", { name: /Import preview/ }, { timeout: 4000 });
}

describe("Open with: a lone .index", () => {
  beforeEach(() => closeImportPreview());

  it("offers Add as source, adds the index and runs the update", async () => {
    const api = new MockApi({ rng: () => 0.5, mobile: true });
    const add = vi.spyOn(api, "addIndex");
    const update = vi.spyOn(api, "updateIndexes");
    const dialog = await openWith(api, [opened("hb.index", index)]);
    expect(within(dialog).getByText(/lists files to download/)).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Install" })).toBeNull();
    const user = userEvent.setup();
    await user.click(within(dialog).getByRole("button", { name: "Add as source" }));
    await user.click(await screen.findByRole("button", { name: "Add source" }));
    await waitFor(() => expect(add).toHaveBeenCalledWith({ url: SOURCE }));
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull());
    expect((await api.listIndexes()).some((i) => i.url === SOURCE)).toBe(true);
  });

  it("keeps the normal preview for an index with files beside it", async () => {
    const api = new MockApi({ rng: () => 0.5, mobile: true });
    const dialog = await openWith(api, [opened("hb.index", index), opened("hb.xml", feat)]);
    expect(within(dialog).getByRole("button", { name: "Install" })).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Add as source" })).toBeNull();
  });
});
