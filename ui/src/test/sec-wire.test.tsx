import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { openBorealisSettings, renderApp } from "./utils";

const codexOnly = (extra: ConstructorParameters<typeof MockApi>[0] = {}) =>
  new MockApi({ aiEnabled: true, aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5, codexOnly: true, ...extra });

describe("consent for outside content on Codex", () => {
  it("asks once per import, then proceeds with the flag set", async () => {
    const user = userEvent.setup();
    const api = codexOnly();
    const sent: Array<boolean | undefined> = [];
    const original = api.importStart.bind(api);
    api.importStart = (req) => { sent.push(req.allowUntrustedWithCodex); return original(req); };
    renderApp("/import", { api });
    await user.type(await screen.findByLabelText("Paste text"), "Runebinder: a half-caster class that etches runes.");
    await user.click(screen.getByRole("button", { name: "Import" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Codex can run shell commands and read files on this computer/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Continue with Codex" }));
    expect(await screen.findByRole("progressbar", { name: "Import progress" })).toBeTruthy();
    expect(sent).toEqual([undefined, true]);
  });

  it("declining keeps the import off Codex and says how to use Claude", async () => {
    const user = userEvent.setup();
    renderApp("/import", { api: codexOnly() });
    await user.type(await screen.findByLabelText("Paste text"), "Some pasted stat block");
    await user.click(screen.getByRole("button", { name: "Import" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Use Claude" }));
    expect(await screen.findByText(/Install or sign in to Claude in AI setup/)).toBeTruthy();
    expect(screen.queryByRole("progressbar", { name: "Import progress" })).toBeNull();
  });

  it("does not ask when Claude is available", async () => {
    const user = userEvent.setup();
    renderApp("/import", { api: new MockApi({ aiEnabled: true, aiChunkDelayMs: 2, importStepMs: 1, rng: () => 0.5 }) });
    await user.type(await screen.findByLabelText("Paste text"), "Some pasted stat block");
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByRole("progressbar", { name: "Import progress" })).toBeTruthy();
  });

  it("rejects with a typed consent_required code at the API level", async () => {
    const api = codexOnly();
    await expect(api.importStart({ sources: [{ kind: "text", text: "x" }] })).rejects.toMatchObject({ code: "consent_required" });
    await expect(api.aiHostEnable()).rejects.toMatchObject({ code: "consent_required" });
    await expect(api.aiHostEnable({ allowUntrustedWithCodex: true })).resolves.toMatchObject({ enabled: true });
  });
});

describe("AI host pairing", () => {
  it("shows the pairing QR, digits and fingerprint in Settings", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: new MockApi({ aiEnabled: true, aiChunkDelayMs: 2, rng: () => 0.5 }) });
    await openBorealisSettings(user, "Phones");
    await user.click(await screen.findByRole("checkbox", { name: "Share AI with phones and tablets" }));
    expect(await screen.findByRole("img", { name: "Pairing QR code" })).toBeTruthy();
    expect(screen.getByText("12345678")).toBeTruthy();
    expect(screen.getByText("ab12 cd34 ef56 7890")).toBeTruthy();
  });

  it("pairing string and fingerprint come with the host status", async () => {
    const api = new MockApi();
    const status = await api.aiHostEnable();
    expect(status.pairingUri).toMatch(/^borealis-pair:/);
    expect(status.fingerprint).toBeTruthy();
    const off = await api.aiHostDisable();
    expect(off.pairingUri).toBeNull();
  });
});

describe("AI program override", () => {
  it("Browse picks through the backend token flow and Clear removes it", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiEnabled: true, aiChunkDelayMs: 2, rng: () => 0.5 });
    const purposes: string[] = [];
    const pickPath = api.pickPath.bind(api);
    api.pickPath = async (a) => { purposes.push(a.purpose); return pickPath(a); };
    api.picks.next = "/opt/tools/codex";
    renderApp("/characters", { api });
    await openBorealisSettings(user, "AI assistant");
    await user.click(screen.getByRole("button", { name: "Browse…" }));
    expect(await screen.findByText("/opt/tools/codex")).toBeTruthy();
    expect(purposes).toEqual(["ai-override"]);
    expect((await api.getSettings()).ai.overridePath).toBe("/opt/tools/codex");
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Clear" })).toBeNull());
    expect((await api.getSettings()).ai.overridePath).toBeNull();
  });

  it("a program that is not claude or codex is refused with a message", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiEnabled: true, aiChunkDelayMs: 2, rng: () => 0.5 });
    api.picks.next = "/usr/bin/python3";
    renderApp("/characters", { api });
    await openBorealisSettings(user, "AI assistant");
    await user.click(screen.getByRole("button", { name: "Browse…" }));
    expect(await screen.findByText(/only the claude or codex command line programs/)).toBeTruthy();
  });

  it("the old auto-update toggle is gone", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await openBorealisSettings(user, "AI assistant");
    expect(screen.queryByText(/Update content automatically/)).toBeNull();
  });
});
