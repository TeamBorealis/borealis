import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Dialog, Flyout } from "../components/controls";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

// Android WebView (Chromium 133) never adds a position:fixed subtree inserted inside a scroll container
// (main.aur-content, overflow:auto) to its accessibility tree once that tree has been read, so screen
// readers and UiAutomator could not see the New character flyout. Overlays are therefore portalled to the
// window root (.aur-window), outside the scroller, next to the shell's own flyouts (android-ax-escalation).
describe("overlay host (android-ax-escalation)", () => {
  it("a page flyout renders outside the scrolling main, as a direct child of the window root", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    const overlay = dialog.parentElement!;
    expect(overlay.classList.contains("flyout-overlay")).toBe(true);
    expect(overlay.closest("main")).toBeNull();
    expect(overlay.parentElement!.classList.contains("aur-window")).toBe(true);
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(screen.getByTestId("new-character-name"));
  });

  it("no overlay is ever left inside the scroll container", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    await screen.findByRole("dialog", { name: "New character" });
    expect(document.querySelector("main .overlay")).toBeNull();
  });

  it("the Explain dialog (a Dialog opened from a page) is portalled out of main too", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    await user.click(await screen.findByRole("button", { name: /^Explain Proficiency, \+\d+$/ }));
    const dialog = await screen.findByRole("dialog", { name: "Explain Proficiency" });
    expect(dialog.closest("main")).toBeNull();
    expect(dialog.parentElement!.parentElement!.classList.contains("aur-window")).toBe(true);
    expect(await within(dialog).findByRole("list", { name: "Contributions" })).toBeTruthy();
  });

  it("without a window root (isolated render) Flyout and Dialog render in place", () => {
    const { container } = render(
      <>
        <Flyout title="Alone" onClose={() => {}}>body</Flyout>
        <Dialog title="Box" onClose={() => {}}>text</Dialog>
      </>,
    );
    expect(container.querySelectorAll(".overlay")).toHaveLength(2);
  });
});
