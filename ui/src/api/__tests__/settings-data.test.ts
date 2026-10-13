import { describe, expect, it } from "vitest";
import { MockApi } from "../mock/MockApi";

describe("settings data files (mock)", () => {
  it("changes folders only through picked tokens", async () => {
    const api = new MockApi();
    expect((await api.getDataDirs()).isDefault).toBe(true);
    await expect(api.setDocumentsDir({ token: "/home/me/elsewhere" })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.saveSettings({ patch: { documentsDir: "/home/me/elsewhere" } })).rejects.toMatchObject({ code: "invalid_argument" });
    const pick = (await api.pickPath({ purpose: "documents-dir" }))[0]!;
    const dirs = await api.setDocumentsDir({ token: pick.token });
    expect(dirs.isDefault).toBe(false);
    expect(dirs.charactersDir).toBe(`${pick.path}/characters`);
    await expect(api.setDocumentsDir({ token: pick.token })).rejects.toMatchObject({ code: "invalid_argument" });
    api.picks.next = "/usr/share";
    const sys = (await api.pickPath({ purpose: "documents-dir" }))[0]!;
    await expect(api.setDocumentsDir({ token: sys.token })).rejects.toMatchObject({ code: "invalid_argument" });
  });

  it("links and unlinks the secondary folder", async () => {
    const api = new MockApi();
    const pick = (await api.pickPath({ purpose: "secondary-content-dir" }))[0]!;
    const linked = await api.setSecondaryContentDir({ token: pick.token });
    expect(linked.dirs.secondaryContentDir).toBe(pick.path);
    expect(linked.report.installed.every((id) => id.startsWith("sec-"))).toBe(true);
    const unlinked = await api.unlinkSecondaryContentDir();
    expect(unlinked.dirs.secondaryContentDir).toBeNull();
    expect(unlinked.report.removed).toEqual(linked.report.installed);
  });

  it("saves the full form on Save and restores the defaults at once", async () => {
    const api = new MockApi();
    const form = { ...(await api.getSettings()), rowHeight: "large" as const, accentColor: "red" as const, autosave: false };
    expect((await api.saveSettings({ patch: form })).rowHeight).toBe("large");
    const restored = await api.restoreDefaultSettings();
    expect(restored.rowHeight).toBe("medium");
    expect(restored.accentColor).toBe("default");
    expect(restored.autosave).toBe(false);
    expect((await api.getSettings()).rowHeight).toBe("medium");
  });

  it("clears caches once and reports the startup update", async () => {
    const api = new MockApi();
    expect((await api.clearCaches()).indexesReset).toBe(1);
    expect((await api.clearCaches()).indexesReset).toBe(0);
    expect((await api.contentUpdateStatus()).state).toBe("done");
  });
});
