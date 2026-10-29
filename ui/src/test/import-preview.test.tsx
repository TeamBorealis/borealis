import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { closeImportPreview } from "../state/importPreview";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });
const feat = (id: string, name: string, grants: string[] = [], extra = "") =>
  `<element name="${name}" type="Feat" source="T" id="${id}">${extra}<rules>${grants.map((g) => `<grant type="Feat" id="${g}"/>`).join("")}</rules></element>`;
const xml = (...els: string[]) => `<elements>${els.join("")}</elements>`;
const file = (name: string, text: string) => new File([text], name);

async function openContent(api: MockApi) {
  renderApp("/content", { api });
  await screen.findByRole("button", { name: "Import files" });
}

function choose(files: File[]) {
  fireEvent.change(screen.getByLabelText("Choose content files"), { target: { files } });
}

describe("Additional Content: import with preview (no AI)", () => {
  beforeEach(() => closeImportPreview());

  it("shows counts, grouped problems and unresolved references, then installs what was previewed", async () => {
    const api = mock();
    const install = vi.spyOn(api, "installImportPreview");
    await openContent(api);
    choose([
      file("a.xml", xml(feat("ID_A", "Alpha"))),
      file("b.xml", xml(feat("ID_B", "Beta", ["ID_A", "ID_GONE"], "<spellcasting/>"))),
    ]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/2 elements from 2 files will be added as "a and 1 more"/)).toBeTruthy();
    expect(within(dialog).getByLabelText("Converted elements").textContent).toContain("Feat2");
    expect(within(dialog).getByText(/0 errors, 1 warning, 0 notes/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: /Warnings \(1\)/ })).toBeTruthy();
    const unresolved = within(dialog).getByLabelText("Unresolved references");
    expect(unresolved.textContent).toContain("ID_GONE");
    expect(unresolved.textContent).not.toContain("ID_A used");
    // issue 8: the referencing file is named, and the copy says what a missing id means
    expect(unresolved.textContent).toContain("used by ID_B (b.xml)");
    expect(within(dialog).getByText(/so what uses them will be missing\. Import the content they come from/)).toBeTruthy();
    expect((await api.listPacks()).some((p) => p.name === "a and 1 more")).toBe(false);

    await userEvent.setup().click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect((await api.listPacks()).some((p) => p.name === "a and 1 more")).toBe(true);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull());
  });

  it("lists ids that requirements only test the absence of apart, as conditions (issue 8)", async () => {
    await openContent(mock());
    choose([file("r.xml", xml(feat("ID_R", "Rider", ["ID_GONE"], "<requirements>!ID_MARKER</requirements>")))]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByLabelText("Unresolved references").textContent).not.toContain("ID_MARKER");
    await userEvent.setup().click(within(dialog).getByRole("button", { name: /Conditions on content you don't have \(1\)/ }));
    expect(within(dialog).getByLabelText("Conditions on content you don't have").textContent).toContain("ID_MARKER used by ID_R (r.xml)");
  });

  it("cancel discards the staged conversion and installs nothing", async () => {
    const api = mock();
    const cancel = vi.spyOn(api, "cancelImportPreview");
    await openContent(api);
    choose([file("a.xml", xml(feat("ID_A", "Alpha")))]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    await userEvent.setup().click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(cancel).toHaveBeenCalledTimes(1));
    expect((await api.listPacks()).some((p) => p.name === "a")).toBe(false);
  });

  it("reports a file with nothing importable and disables Install", async () => {
    await openContent(mock());
    choose([file("empty.xml", "<elements/>")]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/Nothing in empty can be imported/)).toBeTruthy();
    expect((within(dialog).getByRole("button", { name: "Install" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("imports a whole content folder as one set (desktop)", async () => {
    const api = mock();
    api.folderFiles = [
      { fileName: "x.xml", contentBase64: btoa(xml(feat("ID_X", "Ex"))) },
      { fileName: "sub/y.xml", contentBase64: btoa(xml(feat("ID_Y", "Why", ["ID_X"]))) },
    ];
    await openContent(api);
    await userEvent.setup().click(screen.getByRole("button", { name: "Import my content folder" }));
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/2 elements from 2 files will be added as "custom"/)).toBeTruthy();
    expect(within(dialog).queryByLabelText("Unresolved references")).toBeNull();
  });

  it("follows an .index: files it lists but that were not chosen are noted", async () => {
    await openContent(mock());
    choose([
      file("pack.index", `<index><files><file name="a.xml" url="https://x.invalid/a.xml"/><file name="gone.xml" url="https://x.invalid/gone.xml"/></files></index>`),
      file("a.xml", xml(feat("ID_A", "Alpha"))),
    ]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    await userEvent.setup().click(within(dialog).getByRole("button", { name: /Notes \(1\)/ }));
    expect(within(dialog).getByLabelText("Notes").textContent).toContain("gone.xml");
  });

  it("on mobile there is a multi-select picker and no folder button, and no AI wording", async () => {
    await openContent(mock({ mobile: true }));
    expect(screen.queryByRole("button", { name: "Import my content folder" })).toBeNull();
    const input = screen.getByLabelText("Choose content files") as HTMLInputElement;
    expect(input.multiple).toBe(true);
    expect(input.accept).toContain(".zip");
    choose([file("a.xml", xml(feat("ID_A", "Alpha")))]);
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(dialog.textContent).not.toMatch(/\bAI\b|Assistant|Codex|Claude/i);
  });

  it("opens the preview for a file the OS handed to the app", async () => {
    const api = mock({ mobile: true });
    api.openedFiles = [{ fileName: "shared.xml", contentBase64: btoa(xml(feat("ID_S", "Shared"))) }];
    renderApp("/characters", { api });
    const dialog = await screen.findByRole("dialog", { name: /Import preview/ });
    expect(within(dialog).getByText(/1 element from 1 file will be added as "shared"/)).toBeTruthy();
  });

  it("sends a .dnd5e the OS opened to the character importer, not the content preview", async () => {
    const api = mock();
    const importCharacter = vi.spyOn(api, "importCharacter");
    api.openedFiles = [{ fileName: "hero.dnd5e", contentBase64: btoa("<x/>") }];
    renderApp("/characters", { api });
    await waitFor(() => expect(importCharacter).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("dialog", { name: /Import preview/ })).toBeNull();
  });
});
