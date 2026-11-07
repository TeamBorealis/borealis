import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { DEFAULT_SETTINGS } from "../api/mock/default-settings";
import { resetAppUpdate } from "../state/appUpdate";
import { openSettingsTab, renderApp } from "./utils";

// MAS1 (B-190): the Mac App Store build is a desktop store build without the AI CLI integration. The backend says so
// (`settings.storeBuild`, `settings.aiAvailable`); the desktop UI stays, with no AI entry and no update check anywhere.
const AI_TEXT = /\bAI\b|Assistant|Codex|Claude|\bpair/i;
const mas = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5, macAppStore: true, ...extra });

async function openMenu(user: ReturnType<typeof userEvent.setup>, name: string): Promise<string[]> {
  await user.click(await screen.findByRole("menuitem", { name }));
  const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent ?? "");
  await user.keyboard("{Escape}");
  return items;
}

describe("Mac App Store build", () => {
  it("reports a desktop store build without the AI CLI integration", async () => {
    const s = await mas().getSettings();
    expect(s).toMatchObject({ mobile: false, storeBuild: true, aiAvailable: false });
    expect(s.ai.enabled).toBe(false);
  });

  it("keeps AI off even when a settings file or a patch asks for it", async () => {
    const api = mas({ aiEnabled: true });
    expect((await api.getSettings()).ai.enabled).toBe(false);
    await api.saveSettings({ patch: { ai: { ...DEFAULT_SETTINGS.ai, enabled: true }, aiAvailable: true } });
    const s = await api.getSettings();
    expect(s.ai.enabled).toBe(false);
    expect(s.aiAvailable).toBe(false);
  });

  it("has the desktop menu bar without AI entries or Releases", async () => {
    resetAppUpdate();
    const user = userEvent.setup();
    renderApp("/characters", { api: mas({ aiEnabled: true }) });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(AI_TEXT);
    expect((await openMenu(user, "View")).join("|")).not.toMatch(AI_TEXT);
    const help = await openMenu(user, "Help");
    expect(help.join("|")).not.toMatch(AI_TEXT);
    expect(help).not.toContain("Releases");
    expect(help).toEqual(expect.arrayContaining(["News", "Support Page"]));
  });

  it("has no AI, Phones or Updates tab in Application Settings and no AI or update text on any tab", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: mas({ aiEnabled: true }) });
    await openSettingsTab(user, "General");
    const dialog = screen.getByRole("dialog", { name: "Application Settings" });
    const tabs = within(dialog).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "General", "Appearance", "Character build", "Data files", "Equipment", "Character sheet",
      "Rules", "Saving", "Display", "Content", "Search", "Devices", "License", "About",
    ]);
    for (const tab of tabs) {
      await user.click(tab);
      await within(dialog).findByRole("tabpanel");
      expect(dialog.textContent).not.toMatch(AI_TEXT);
      expect(within(dialog).queryByRole("checkbox", { name: "Check for new versions of Borealis" })).toBeNull();
      expect(within(dialog).queryByRole("button", { name: "Check now" })).toBeNull();
    }
  });

  it.each(["/ai", "/setup/ai", "/import"])("%s redirects to the characters", async (route) => {
    renderApp(route, { api: mas({ aiEnabled: true }) });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
  });

  it("asks to pair again a device whose pairing stayed with the website version", async () => {
    const user = userEvent.setup();
    const day = 1_793_000_000;
    const device = (id: string, name: string, needsPairing?: boolean) => ({ id, name, platform: "ios" as const, pairedAt: day, lastSyncedAt: day, expiresAt: day + 180 * 86_400, fingerprint: "9b12 89ce 4fd2 8eff", ...(needsPairing ? { needsPairing } : {}) });
    renderApp("/characters", { api: mas({ linkDevices: [device("a1b2c3d4e5f60718", "Phone", true), device("0f1e2d3c4b5a6978", "Tablet")] }) });
    await openSettingsTab(user, "Devices");
    const phone = await screen.findByRole("group", { name: "Phone" });
    expect(within(phone).getByText(/This pairing did not move to this version of Borealis/)).toBeTruthy();
    expect(within(screen.getByRole("group", { name: "Tablet" })).queryByText(/did not move/)).toBeNull();
  });

  it("gives the neutral import message with no AI text for a PDF", async () => {
    renderApp("/characters", { api: mas() });
    await screen.findByRole("heading", { name: "Characters" });
    fireEvent.drop(window, { dataTransfer: { types: ["Files"], files: [new File(["%PDF"], "sheet.pdf", { type: "application/pdf" })] } });
    const msg = await screen.findByText(/sheet\.pdf cannot be imported here/);
    expect(msg.textContent).not.toMatch(AI_TEXT);
  });
});
