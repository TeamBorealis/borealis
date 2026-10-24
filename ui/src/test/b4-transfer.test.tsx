import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { LinkStatus } from "../api/types";
import { isNewer, statusLine, summaryText } from "../pages/transfer/messages";
import { type Scanner, setScannerForTests } from "../pages/transfer/scanner";
import { renderApp } from "./utils";

/** The mock's event bus (private: tests push phases the simulated peer never produces). */
const emit = (api: MockApi, s: LinkStatus) => (api as unknown as { emit: (e: "link:status", p: LinkStatus) => void }).emit("link:status", s);

/** Records session ids as the mock emits them (the simulated peer acts on the newest one). */
function track(api: MockApi) {
  const seen: LinkStatus[] = [];
  api.subscribe("link:status", (s) => seen.push(s));
  return { last: () => seen[seen.length - 1]!, seen };
}

async function openFile(user: ReturnType<typeof userEvent.setup>, entry: string) {
  await user.click(await screen.findByRole("menuitem", { name: "File" }));
  await user.click(await screen.findByRole("menuitem", { name: entry }));
}

const statusBar = () => screen.getByRole("contentinfo", { name: "Status" });
const scannerReturning = (r: Awaited<ReturnType<Scanner["scan"]>>): Scanner => ({ scan: async () => r, cancel: async () => undefined });

afterEach(() => {
  setScannerForTests(null);
  vi.restoreAllMocks();
  try { localStorage.removeItem("borealis.localNetworkExplained"); } catch { /* ignore */ }
});

describe("B4 Send to device / Receive from device (desktop)", () => {
  it("File menu lists the device entries and the bundle entries after the classic items, before Exit", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "File" }));
    const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent);
    expect(items.slice(-5)).toEqual(["Send to device...", "Receive from device...", "Export to bundle...", "Open bundle...", "Exit Application"]);
  });

  it("Receive: shows QR, code and fingerprint; the phone sends; preview with per-item actions; Accept reports in the status bar", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const t = track(api);
    renderApp("/characters", { api });
    await openFile(user, "Receive from device...");
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    expect(await within(dlg).findByRole("img", { name: "Connection QR code" })).toBeTruthy();
    expect(within(dlg).getByLabelText("Code").textContent).toBe("42-ember-lantern");
    expect(dlg.textContent).toContain("ab12 cd34 ef56 7890");
    expect(dlg.textContent).toContain("expires in 5:00");
    expect(within(dlg).getAllByRole("button").map((b) => b.textContent).slice(-1)).toEqual(["Cancel"]);

    act(() => { api.link.simulatePeer(t.last().sessionId); });
    const rows = within(await within(dlg).findByRole("list", { name: "What Accept changes" })).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringMatching(/^Mira\d+ KBNew$/),
      expect.stringMatching(/^Tobin Ashgrove\d+ KBReplaces your older copy$/),
      expect.stringMatching(/^Tavern Feats \(homebrew\)\d+ KBYours is newer: kept$/),
    ]);
    await user.click(within(dlg).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Receive from device" })).toBeNull());
    expect(statusBar().textContent).toContain("Received from Borealis on iPhone or iPad: 1 new, 1 replaced, 1 kept (yours was newer).");
    expect(await screen.findByRole("button", { name: /^Mira, / })).toBeTruthy();
  });

  it("Send: tick characters and packs, Show code, the phone accepts, result in the status bar", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const t = track(api);
    const start = vi.spyOn(api, "linkStartSend");
    renderApp("/characters", { api });
    await openFile(user, "Send to device...");
    const dlg = await screen.findByRole("dialog", { name: "Send to device" });
    const show = within(dlg).getByRole("button", { name: "Show code" });
    expect((show as HTMLButtonElement).disabled).toBe(true);
    await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.selectOptions(within(dlg).getByLabelText("CONTENT PACKS"), "all");
    await user.click(show);
    expect(await within(dlg).findByRole("img", { name: "Connection QR code" })).toBeTruthy();
    const ids = start.mock.calls[0]![0].selection.characterIds;
    expect(ids).toHaveLength(1);
    expect(start.mock.calls[0]![0].selection.packs).toBe("all");

    act(() => { api.link.simulatePeer(t.last().sessionId); });
    expect(await within(dlg).findByText(/Waiting for Borealis on iPhone or iPad to accept/)).toBeTruthy();
    act(() => { api.link.simulatePeer(t.last().sessionId); });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Send to device" })).toBeNull());
    expect(statusBar().textContent).toContain("Sent to Borealis on iPhone or iPad: 1 new, 1 replaced, 1 kept (theirs was newer).");
  });

  it("PC to PC: Send types the receiving computer's code", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const t = track(api);
    const connect = vi.spyOn(api, "linkConnect");
    renderApp("/characters", { api });
    await openFile(user, "Send to device...");
    const dlg = await screen.findByRole("dialog", { name: "Send to device" });
    await user.click(await within(dlg).findByRole("checkbox", { name: "All characters" }));
    await user.click(within(dlg).getByRole("button", { name: "Enter a code" }));
    const connectBtn = within(dlg).getByRole("button", { name: "Connect" }) as HTMLButtonElement;
    expect(connectBtn.disabled).toBe(true);
    await user.type(within(dlg).getByLabelText("CODE"), "42-ember-lantern");
    await user.click(connectBtn);
    expect(await within(dlg).findByText(/Waiting for .* to accept/)).toBeTruthy();
    expect(connect.mock.calls[0]![0]).toMatchObject({ target: "42-ember-lantern", host: undefined });
    expect(connect.mock.calls[0]![0].selection!.characterIds.length).toBeGreaterThan(1);
    act(() => { api.link.simulatePeer(t.last().sessionId); });
    await waitFor(() => expect(statusBar().textContent).toMatch(/^Sent to /));
  });

  it("a malformed code is explained in the dialog; Cancel closes a waiting window and says so", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const cancel = vi.spyOn(api, "linkCancel");
    renderApp("/characters", { api });
    await openFile(user, "Receive from device...");
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    await within(dlg).findByRole("img", { name: "Connection QR code" });
    await user.click(within(dlg).getByRole("button", { name: "Enter a code instead" }));
    await user.type(within(dlg).getByLabelText("CODE"), "ember lantern");
    await user.click(within(dlg).getByRole("button", { name: "Connect" }));
    expect((await within(dlg).findByRole("alert")).textContent).toContain("That code is not valid");
    await user.click(within(dlg).getByRole("button", { name: "Show a code" }));
    await within(dlg).findByRole("img", { name: "Connection QR code" });
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Receive from device" })).toBeNull();
    expect(cancel).toHaveBeenCalled();
    expect(statusBar().textContent).toContain("Transfer cancelled. Nothing was changed.");
  });

  it("an expired code offers a new one; a failure offers Try again", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const t = track(api);
    const start = vi.spyOn(api, "linkStartReceive");
    renderApp("/characters", { api });
    await openFile(user, "Receive from device...");
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    await within(dlg).findByRole("img", { name: "Connection QR code" });
    act(() => { emit(api, { ...t.last(), phase: "expired", qrPayload: null, code: null }); });
    await user.click(await within(dlg).findByRole("button", { name: "New code" }));
    await waitFor(() => expect(start).toHaveBeenCalledTimes(2));
    await within(dlg).findByRole("img", { name: "Connection QR code" });
    act(() => { emit(api, { ...t.last(), phase: "failed", errorCode: "wrong_code", error: "x" }); });
    expect((await within(dlg).findByRole("alert")).textContent).toContain("the code did not match");
    expect(statusBar().textContent).toContain("Transfer failed: the code did not match");
    expect(within(dlg).getByRole("button", { name: "Try again" })).toBeTruthy();
  });
});

describe("B4 on phones (More > Send / Receive)", () => {
  const phone = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, mobile: true });

  it("Receive: camera denied falls back to the typed code + address", async () => {
    const user = userEvent.setup();
    const api = phone();
    setScannerForTests(scannerReturning({ kind: "denied" }));
    const connect = vi.spyOn(api, "linkConnect");
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Receive from device" }));
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    await user.click(within(dlg).getByRole("button", { name: "Scan the code on the other device" }));
    expect((await within(dlg).findByText(/Camera access is off for Borealis/)).textContent).toBeTruthy();
    const connectBtn = within(dlg).getByRole("button", { name: "Connect" }) as HTMLButtonElement;
    await user.type(within(dlg).getByLabelText("CODE"), "42-ember-lantern");
    expect(connectBtn.disabled).toBe(true); // phones need the address too (no discovery)
    await user.type(within(dlg).getByLabelText("ADDRESS"), "192.168.1.20:41234");
    await user.click(connectBtn);
    expect(connect.mock.calls[0]![0]).toMatchObject({ target: "42-ember-lantern", host: "192.168.1.20:41234", selection: undefined });
    await user.click(await within(dlg).findByRole("button", { name: "Accept" }));
    await waitFor(() => expect(statusBar().textContent).toContain("Received from Borealis on Mac"));
  });

  it("Send: pick, scan the computer's Receive QR, done; no camera shows the type fallback", async () => {
    const user = userEvent.setup();
    const api = phone();
    const t = track(api);
    let scanned = "";
    setScannerForTests({ scan: async (expect) => { scanned = expect; return { kind: "ok", text: "borealis-link:v1?h=192.168.1.20:41234&fp=q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA&c=AAECAwQFBgcICQoLDA0ODw&n=Borealis%20on%20Mac&m=receive" }; }, cancel: async () => undefined });
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Send to device" }));
    const dlg = await screen.findByRole("dialog", { name: "Send to device" });
    await user.click(await within(dlg).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.click(within(dlg).getByRole("button", { name: "Next" }));
    await user.click(within(dlg).getByRole("button", { name: "Scan the code on the other device" }));
    expect(await within(dlg).findByText(/Waiting for Borealis on Mac to accept/)).toBeTruthy();
    expect(scanned).toBe("receive");
    act(() => { api.link.simulatePeer(t.last().sessionId); });
    await waitFor(() => expect(statusBar().textContent).toContain("Sent to Borealis on Mac"));

    setScannerForTests(scannerReturning({ kind: "unavailable", reason: "No camera available on this device" }));
    await user.click(screen.getByRole("button", { name: "Send to device" }));
    const again = await screen.findByRole("dialog", { name: "Send to device" });
    await user.click(await within(again).findByRole("checkbox", { name: "Seraphine Vale" }));
    await user.click(within(again).getByRole("button", { name: "Next" }));
    await user.click(within(again).getByRole("button", { name: "Scan the code on the other device" }));
    expect(await within(again).findByText(/This device cannot scan a code here/)).toBeTruthy();
  });

  it("a scanned QR for the wrong direction is explained", async () => {
    const user = userEvent.setup();
    setScannerForTests(scannerReturning({ kind: "ok", text: "borealis-link:v1?h=1.2.3.4:5&fp=q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA&c=AAECAwQFBgcICQoLDA0ODw&n=X&m=receive" }));
    renderApp("/more", { api: phone() });
    await user.click(await screen.findByRole("button", { name: "Receive from device" }));
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    await user.click(within(dlg).getByRole("button", { name: "Scan the code on the other device" }));
    expect((await within(dlg).findByRole("alert")).textContent).toContain("waiting to receive");
  });

  it("iOS: the Local Network prompt is explained once, just before the first connection", async () => {
    const user = userEvent.setup();
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (iPhone; CPU iPhone OS 27_0 like Mac OS X)");
    const api = phone();
    const start = vi.spyOn(api, "linkStartReceive");
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Receive from device" }));
    const dlg = await screen.findByRole("dialog", { name: "Receive from device" });
    await user.click(within(dlg).getByRole("button", { name: "Show a code on this device" }));
    const note = await screen.findByRole("dialog", { name: "Local network access" });
    expect(note.textContent).toContain("choose Allow");
    expect(start).not.toHaveBeenCalled();
    await user.click(within(note).getByRole("button", { name: "Continue" }));
    expect(await within(dlg).findByRole("img", { name: "Connection QR code" })).toBeTruthy();
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Receive from device" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Receive from device" })).getByRole("button", { name: "Show a code on this device" }));
    expect(screen.queryByRole("dialog", { name: "Local network access" })).toBeNull();
  });
});

describe("C1 Borealis Settings > Devices", () => {
  it("lists paired devices with last synced and expiry; Remove asks, revokes and reports", async () => {
    const user = userEvent.setup();
    const day = Date.UTC(2026, 9, 24) / 1000;
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, linkDevices: [
      { id: "a1b2c3d4e5f60718", name: "Borealis on iPhone or iPad", platform: "ios", pairedAt: day - 86_400, lastSyncedAt: day, expiresAt: day + 180 * 86_400 },
      { id: "0f1e2d3c4b5a6978", name: "Borealis on Windows PC", platform: "windows", pairedAt: day - 30 * 86_400, lastSyncedAt: day - 3 * 86_400, expiresAt: day + 177 * 86_400 },
    ] });
    const forget = vi.spyOn(api, "linkForgetDevice");
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Borealis Settings" }));
    await user.click(await screen.findByRole("tab", { name: "Devices" }));
    const phoneRow = await screen.findByRole("group", { name: "Borealis on iPhone or iPad" });
    expect(phoneRow.textContent).toContain("iPhone or iPad · Last synced 24 Oct 2026 · Expires 22 Apr 2027");
    expect(screen.getByRole("group", { name: "Borealis on Windows PC" }).textContent).toContain("Windows PC · Last synced 21 Oct 2026");
    await user.click(within(phoneRow).getByRole("button", { name: "Remove" }));
    const confirm = await screen.findByRole("dialog", { name: "Remove device" });
    await user.click(within(confirm).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("group", { name: "Borealis on iPhone or iPad" })).toBeNull());
    expect(forget).toHaveBeenCalledWith({ deviceId: "a1b2c3d4e5f60718" });
    expect(statusBar().textContent).toContain("Removed 'Borealis on iPhone or iPad' from your devices.");
  });

  it("explains how to add one when the list is empty", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Borealis Settings" }));
    await user.click(await screen.findByRole("tab", { name: "Devices" }));
    expect(await screen.findByText(/No devices yet\. Use File > Send to device/)).toBeTruthy();
  });
});

describe("transfer messages", () => {
  it("summaries, status lines and phase ordering", () => {
    expect(summaryText({ added: 2, replaced: 0, keptLocal: 0, identical: 1 }, true)).toBe("2 new, 1 already there");
    expect(summaryText({ added: 0, replaced: 0, keptLocal: 0, identical: 0 }, true)).toBe("nothing changed");
    const base = { sessionId: "s", role: "send", phase: "declined", peerName: "PC" } as LinkStatus;
    expect(statusLine(base)).toBe("PC declined the transfer.");
    expect(statusLine({ ...base, role: "receive" })).toBe("Transfer declined. Nothing was written.");
    expect(statusLine({ ...base, phase: "preview" })).toBeNull();
    expect(isNewer({ ...base, phase: "preview" }, { ...base, phase: "connecting" })).toBe(false);
    expect(isNewer({ ...base, phase: "connecting" }, { ...base, phase: "preview" })).toBe(true);
    expect(isNewer({ ...base, phase: "done" }, { ...base, phase: "transferring" })).toBe(false);
  });
});
