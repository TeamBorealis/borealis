import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { bundleExportedLine, bundleOpenedLine, bundleSharedLine } from "../pages/transfer/messages";
import { renderApp } from "./utils";

async function openFile(user: ReturnType<typeof userEvent.setup>, entry: string) {
  await user.click(await screen.findByRole("menuitem", { name: "File" }));
  await user.click(await screen.findByRole("menuitem", { name: entry }));
}

const statusBar = () => screen.getByRole("contentinfo", { name: "Status" });

describe("A2 bundles as files (desktop)", () => {
  it("File menu offers Export to bundle... and Open bundle... after the device entries", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "File" }));
    const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent);
    expect(items.slice(-5)).toEqual(["Send to device...", "Receive from device...", "Export to bundle...", "Open bundle...", "Exit Application"]);
  });

  it("Export to bundle: pick a character, Export..., the file is written and the status bar says so", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const exportSpy = vi.spyOn(api, "bundleExport");
    renderApp("/characters", { api });
    await openFile(user, "Export to bundle...");
    const dlg = await screen.findByRole("dialog", { name: "Export to bundle" });
    const exportBtn = within(dlg).getByRole("button", { name: "Export..." }) as HTMLButtonElement;
    expect(exportBtn.disabled).toBe(true);
    expect(dlg.textContent).toContain("Choose what goes into the file.");
    await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.selectOptions(within(dlg).getByLabelText("CONTENT PACKS"), "none");
    await user.click(exportBtn);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Export to bundle" })).toBeNull());
    expect(exportSpy.mock.calls[0]![0].selection.packs).toBe("none");
    expect(statusBar().textContent).toContain("Exported 1 character to Seraphine Vale.borealis.");
    expect(api.bundles.exported[0]!.path).toBe("/home/me/Documents/Seraphine Vale.borealis");
  });

  it("a cancelled save dialog keeps the export dialog open and writes nothing", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    api.bundles.nextSavePath = null;
    renderApp("/characters", { api });
    await openFile(user, "Export to bundle...");
    const dlg = await screen.findByRole("dialog", { name: "Export to bundle" });
    await user.click(await within(dlg).findByRole("checkbox", { name: "All characters" }));
    await user.click(within(dlg).getByRole("button", { name: "Export..." }));
    await waitFor(() => expect(api.bundles.exported).toHaveLength(1));
    expect(api.bundles.exported[0]!.path).toBeNull();
    expect(screen.getByRole("dialog", { name: "Export to bundle" })).toBeTruthy();
  });

  it("Open bundle: the same preview as a transfer, Accept applies it and refreshes the characters", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const accept = vi.spyOn(api, "bundleAccept");
    renderApp("/characters", { api });
    await openFile(user, "Open bundle...");
    const dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    expect(dlg.textContent).toContain("Mira.borealis from Borealis on Mac holds 3 items");
    const rows = within(within(dlg).getByRole("list", { name: "What Accept changes" })).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringMatching(/^Mira\d+ KBNew$/),
      expect.stringMatching(/^Tobin Ashgrove\d+ KBReplaces your older copy$/),
      expect.stringMatching(/^Tavern Feats \(homebrew\)\d+ KBYours is newer: kept$/),
    ]);
    expect(dlg.textContent).toContain("The most recently edited copy wins.");
    expect(accept).not.toHaveBeenCalled();
    await user.click(within(dlg).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Open bundle" })).toBeNull());
    expect(statusBar().textContent).toContain("Opened Mira.borealis: 1 new, 1 replaced, 1 kept (yours was newer).");
    expect(await screen.findByRole("button", { name: /^Mira, / })).toBeTruthy();
  });

  it("Cancel discards the bundle and writes nothing", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const discard = vi.spyOn(api, "bundleDiscard");
    const accept = vi.spyOn(api, "bundleAccept");
    renderApp("/characters", { api });
    await openFile(user, "Open bundle...");
    const dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Open bundle" })).toBeNull());
    expect(discard).toHaveBeenCalledTimes(1);
    expect(accept).not.toHaveBeenCalled();
    expect(statusBar().textContent).toContain("Open bundle cancelled. Nothing was changed.");
  });

  it("a damaged or foreign file shows the plain message and nothing is applied", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    api.picks.next = "/home/me/Downloads/tampered.borealis";
    const accept = vi.spyOn(api, "bundleAccept");
    renderApp("/characters", { api });
    await openFile(user, "Open bundle...");
    const dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    expect(dlg.textContent).toContain("tampered.borealis");
    expect(dlg.textContent).toContain("This bundle cannot be used: it is damaged or was changed after it was made. Nothing was changed.");
    await user.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Open bundle" })).toBeNull());
    expect(accept).not.toHaveBeenCalled();
  });

  it("a file dropped on the window opens the preview; two files show one after the other", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    renderApp("/characters", { api });
    await screen.findByRole("menuitem", { name: "File" });
    act(() => { api.dropFiles([{ name: "A.borealis", contentBase64: "" }, { name: "B.borealis", contentBase64: "" }, { name: "x.txt", contentBase64: "" }]); });
    let dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    expect(dlg.textContent).toContain("A.borealis");
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    await waitFor(() => expect(dlg.textContent).toContain("B.borealis"));
  });

  it("a bundle the OS opened with the app shows the preview", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    api.bundles.queueOpened("/Users/me/Downloads/From phone.borealis");
    renderApp("/characters", { api });
    const dlg = await screen.findByRole("dialog", { name: "Open bundle" });
    expect(dlg.textContent).toContain("From phone.borealis");
  });

  it("status lines read as sentences", () => {
    expect(bundleOpenedLine("Mira.borealis", { added: 0, replaced: 0, keptLocal: 0, identical: 1 })).toBe("Opened Mira.borealis: 1 already there.");
    expect(bundleExportedLine("a.borealis", 2, 1)).toBe("Exported 2 characters and 1 content pack to a.borealis.");
    expect(bundleSharedLine("a.borealis")).toBe("Shared a.borealis. Open it with Borealis on the other device.");
  });
});

describe("A2 share sheet (phone)", () => {
  async function openMore(user: ReturnType<typeof userEvent.setup>, api: MockApi) {
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Share as a file" }));
    return screen.findByRole("dialog", { name: "Share as a file" });
  }

  it("Android: Share opens the system share sheet from the backend and closes the dialog", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, mobile: true });
    api.bundles.androidShare = true;
    const dlg = await openMore(user, api);
    await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.click(within(dlg).getByRole("button", { name: "Share" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Share as a file" })).toBeNull());
    expect(api.bundles.shared).toHaveLength(1);
    expect(statusBar().textContent).toContain("Shared Seraphine Vale.borealis.");
  });

  it("iOS: the bundle waits for a fresh tap, then goes through the web view's share sheet", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, mobile: true });
    const share = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { share, canShare: () => true });
    try {
      const dlg = await openMore(user, api);
      await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
      await user.click(within(dlg).getByRole("button", { name: "Share" }));
      expect(await within(dlg).findByText(/is ready/)).toBeTruthy();
      expect(share).not.toHaveBeenCalled();
      await user.click(within(dlg).getByRole("button", { name: "Share" }));
      await waitFor(() => expect(share).toHaveBeenCalledTimes(1));
      expect(share.mock.calls[0]![0].files[0].name).toBe("Seraphine Vale.borealis");
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Share as a file" })).toBeNull());
      expect(statusBar().textContent).toContain("Shared Seraphine Vale.borealis.");
    } finally {
      Reflect.deleteProperty(navigator, "share");
      Reflect.deleteProperty(navigator, "canShare");
    }
  });

  it("the Send dialog on a phone also offers Share as a file", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, mobile: true });
    api.bundles.androidShare = true;
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Send to device" }));
    const dlg = await screen.findByRole("dialog", { name: "Send to device" });
    await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.click(within(dlg).getByRole("button", { name: "Share as a file" }));
    await waitFor(() => expect(api.bundles.shared).toHaveLength(1));
  });
});
