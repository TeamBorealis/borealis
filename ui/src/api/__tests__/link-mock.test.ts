import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import type { LinkStatus } from "../types";

describe("mock device transfer", () => {
  it("receive by QR: preview, accept, done, with status events", async () => {
    const api = new MockApi({ seed: false });
    const seen: LinkStatus[] = [];
    api.subscribe("link:status", (s) => seen.push(s));
    const shown = await api.linkStartSend({ selection: { characterIds: ["c_1"] } });
    expect(shown.phase).toBe("waiting");
    expect(shown.qrPayload).toMatch(/^borealis-link:v1\?.*&m=send$/);
    expect(shown.code).toBe("42-ember-lantern");
    const scan = await api.linkConnect({ target: shown.qrPayload! });
    expect(scan.phase).toBe("preview");
    expect(scan.preview?.items[0]).toMatchObject({ label: "Mira", action: "new" });
    const done = await api.linkAccept({ sessionId: scan.sessionId });
    expect(done.phase).toBe("done");
    expect(done.summary?.added).toBe(1);
    expect(seen.map((s) => s.phase)).toContain("transferring");
  });

  it("refuses a QR for the wrong direction and a malformed code", async () => {
    const api = new MockApi({ seed: false });
    const shown = await api.linkStartReceive();
    await expect(api.linkConnect({ target: shown.qrPayload! })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.linkConnect({ target: "ember lantern" })).rejects.toMatchObject({ code: "invalid_argument" });
    const typed = await api.linkConnect({ target: "42-ember-lantern", host: "192.168.1.20:41234", selection: { characterIds: ["c_1"] } });
    expect(typed.phase).toBe("waiting_for_peer");
    expect((await api.linkCancel({ sessionId: shown.sessionId })).phase).toBe("cancelled");
  });

  it("a simulated peer answers an open window, the receive lands in the library and pairs the device", async () => {
    const api = new MockApi({ seed: false });
    const shown = await api.linkStartReceive();
    const preview = api.link.simulatePeer(shown.sessionId);
    expect(preview.phase).toBe("preview");
    expect(preview.preview?.items.map((i) => i.action)).toEqual(["new", "replace", "keep_local"]);
    const done = await api.linkAccept({ sessionId: shown.sessionId });
    expect(done.summary).toEqual({ added: 1, replaced: 1, keptLocal: 1, identical: 0 });
    expect((await api.listCharacters()).map((c) => c.name)).toEqual(["Mira"]);
    const devices = await api.linkDevices();
    expect(devices).toHaveLength(1);
    expect(devices[0]!.expiresAt - devices[0]!.lastSyncedAt).toBe(180 * 86_400);

    const sending = await api.linkStartSend({ selection: { characterIds: ["c_1"] } });
    expect(api.link.simulatePeer(sending.sessionId).phase).toBe("waiting_for_peer");
    expect(api.link.simulatePeer(sending.sessionId).phase).toBe("done");
  });

  it("devices: expired pairings drop out, Remove forgets one, malformed ids are refused", async () => {
    const now = 1_800_000_000;
    const dev = (id: string, lastSyncedAt: number) => ({ id, name: id, platform: "android" as const, pairedAt: 1, lastSyncedAt, expiresAt: lastSyncedAt + 180 * 86_400, fingerprint: "0000 0000 0000 0000" });
    const api = new MockApi({ seed: false, linkDevices: [dev("00000000000000aa", now - 200 * 86_400), dev("00000000000000bb", now - 10), dev("00000000000000cc", now)] });
    (api.link as unknown as { now: () => number }).now = () => now;
    expect((await api.linkDevices()).map((d) => d.id)).toEqual(["00000000000000cc", "00000000000000bb"]);
    expect((await api.linkForgetDevice({ deviceId: "00000000000000cc" })).map((d) => d.id)).toEqual(["00000000000000bb"]);
    await expect(api.linkForgetDevice({ deviceId: "../x" })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});
