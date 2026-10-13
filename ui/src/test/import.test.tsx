import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled: true, rng: () => 0.5 });

describe("import anything", () => {
  it("drops text, streams progress, revises one element and finalizes", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/import", { api: fast() });
    await user.type(await screen.findByLabelText("Paste text"), "Runebinder: a half-caster class that etches runes.");
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByRole("progressbar", { name: "Import progress" })).toBeTruthy();
    expect(await screen.findByText("Level 1–20 test passed ✓", {}, { timeout: 3000 })).toBeTruthy();

    const capstone = screen.getByRole("region", { name: "Master Runebinder" });
    expect(within(capstone).getByText("Needs review")).toBeTruthy();
    expect(within(capstone).getByText(/Balance: Capstone/)).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Rune Etching" })).getByText("Auto-fixed")).toBeTruthy();

    await user.type(within(capstone).getByLabelText("Fix Master Runebinder with AI"), "limit to once per long rest");
    await user.click(within(capstone).getByRole("button", { name: "Fix with AI" }));
    await waitFor(() => expect(within(screen.getByRole("region", { name: "Master Runebinder" })).getByText("OK")).toBeTruthy());

    await user.click(screen.getByRole("button", { name: "Add to Library" }));
    expect(await screen.findByRole("button", { name: "Open in Compendium" })).toBeTruthy();
    const packs = await api.listPacks();
    expect(packs.some((p) => p.name === "Runebinder (imported)" && p.redistributable === false)).toBe(true);
  });

  it("rejects invalid raw JSON in the editor", async () => {
    const user = userEvent.setup();
    renderApp("/import", { api: fast() });
    await user.type(await screen.findByLabelText("Describe it"), "a feat that lets you jump");
    await user.click(screen.getByRole("button", { name: "Feat" }));
    await user.click(screen.getByRole("button", { name: "Import" }));
    const card = await screen.findByRole("region", { name: /Imported feat/ }, { timeout: 3000 });
    await user.click(within(card).getByRole("button", { name: /raw JSON/ }));
    fireEvent.change(within(card).getByLabelText(/Raw JSON for/), { target: { value: "{ nope" } });
    await user.click(within(card).getByRole("button", { name: "Validate and save" }));
    expect((await within(card).findByRole("alert")).textContent).toMatch(/JSON/i);
  });

  it("asks for the parent class when a subclass is ambiguous and blocks until chosen", async () => {
    const user = userEvent.setup();
    renderApp("/import", { api: fast() });
    await user.type(await screen.findByLabelText("Paste text"), "Path of the Stormcaller subclass: at 3rd level you gain a storm aura.");
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByText("Which class is this subclass for?", {}, { timeout: 3000 })).toBeTruthy();
    expect((screen.getByRole("button", { name: "Add to Library" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("radio", { name: "Barbarian" }));
    await user.click(screen.getByRole("button", { name: "Use this class" }));
    await waitFor(() => expect(screen.queryByText("Which class is this subclass for?")).toBeNull());
    await waitFor(() => expect((screen.getByRole("button", { name: "Add to Library" }) as HTMLButtonElement).disabled).toBe(false));
  });

  it("renders the camera input for mobile and is reachable from Content", async () => {
    const user = userEvent.setup();
    renderApp("/content", { api: fast() });
    await user.click(await within(await screen.findByRole("main")).findByRole("link", { name: "Import" }));
    const cam = (await screen.findByLabelText("Take photo")) as HTMLInputElement;
    expect(cam.getAttribute("capture")).toBe("environment");
    expect(cam.getAttribute("accept")).toBe("image/*");
  });
});
