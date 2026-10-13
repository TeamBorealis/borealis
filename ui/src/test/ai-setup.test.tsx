import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { openBorealisSettings, renderApp } from "./utils";

const fresh = (aiSetup: ConstructorParameters<typeof MockApi>[0] extends infer O ? (O extends { aiSetup?: infer S } ? S : never) : never) =>
  new MockApi({ aiChunkDelayMs: 2, aiEnabled: true, rng: () => 0.5, aiSetup });

describe("first-run AI setup", () => {
  it("never redirects to /setup/ai on first launch; the wizard is only reachable once AI is on", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, aiSetup: { completed: false } });
    renderApp("/characters", { api });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Set up AI assistance" })).toBeNull();
  });

  it("walks install (with consent) then sign-in", async () => {
    const user = userEvent.setup();
    const api = fresh({ completed: false });
    renderApp("/setup/ai", { api });
    expect(await screen.findByRole("heading", { name: "Set up AI assistance" })).toBeTruthy();
    const claude = await screen.findByRole("region", { name: "Claude Code" });
    expect(within(claude).getByText("Not installed")).toBeTruthy();

    // The pinned command is shown up front; there is no second dialog in the page.
    expect(within(claude).getByText("npm install -g @anthropic-ai/claude-code@2.1.293")).toBeTruthy();
    await user.click(within(claude).getByRole("button", { name: "Install Claude Code" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await within(claude).findByText(/added 1 package/)).toBeTruthy();
    await waitFor(() => expect(within(claude).getByText("Sign-in needed")).toBeTruthy());

    // Sign in shows the exact command; Borealis handles no credentials.
    await user.click(within(claude).getByRole("button", { name: "Sign in" }));
    expect(await within(claude).findByText("claude auth login")).toBeTruthy();
    expect(within(claude).getByRole("button", { name: "Copy command" })).toBeTruthy();
    await user.click(within(claude).getByRole("button", { name: "I have signed in, check again" }));
    await waitFor(() => expect(within(claude).getByText("Ready")).toBeTruthy());

    await user.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Set up AI assistance" })).toBeNull());
    expect((await api.aiSetupStatus()).completed).toBe(true);
  });

  it("can be skipped and does not reappear", async () => {
    const user = userEvent.setup();
    const api = fresh({ completed: false });
    renderApp("/setup/ai", { api });
    await user.click(await screen.findByRole("button", { name: "Skip — use Borealis without AI" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Set up AI assistance" })).toBeNull());
    expect((await api.aiSetupStatus()).completed).toBe(true);
  });

  it("shows manual instructions instead of an install button when npm is missing", async () => {
    const api = fresh({ completed: true, hasNpm: false });
    renderApp("/setup/ai", { api });
    const codex = await screen.findByRole("region", { name: "Codex CLI" });
    expect(within(codex).queryByRole("button", { name: /Install/ })).toBeNull();
    expect(within(codex).getByText(/npm was not found/)).toBeTruthy();
  });

  it("is reachable from Settings and the mock refuses installs without consent", async () => {
    const user = userEvent.setup();
    const api = fresh({ completed: true });
    renderApp("/characters", { api });
    await openBorealisSettings(user, "AI assistant");
    expect((screen.getByRole("checkbox", { name: "Enable AI assistant (optional)" }) as HTMLInputElement).checked).toBe(true);
    await user.click(await screen.findByRole("button", { name: "AI setup…" }));
    expect(await screen.findByRole("heading", { name: "Set up AI assistance" })).toBeTruthy();
    await expect(api.aiSetupInstall({ provider: "codex", consentToken: "forged" })).rejects.toMatchObject({ code: "requirement_failed" });
  });

  it("declining the native confirmation installs nothing and shows no error", async () => {
    const user = userEvent.setup();
    const api = fresh({ completed: true, declineNativeConfirm: true });
    renderApp("/setup/ai", { api });
    const claude = await screen.findByRole("region", { name: "Claude Code" });
    await user.click(within(claude).getByRole("button", { name: "Install Claude Code" }));
    await waitFor(() => expect(within(claude).getByRole("button", { name: "Install Claude Code" })).toBeTruthy());
    expect(within(claude).queryByRole("alert")).toBeNull();
    expect((await api.aiSetupStatus()).providers[0]!.installed).toBe(false);
  });
});
