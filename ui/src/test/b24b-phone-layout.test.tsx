import { screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "./utils";

const css = (file: string) => readFileSync(resolve(__dirname, "..", file), "utf8");
const phoneBlock = (text: string) => text.slice(text.indexOf("@media (max-width: 899px) {"));

describe("Phone layout (390x844 Playwright check: tab fully visible, Import files inside the viewport, source column kept)", () => {
  afterEach(() => { vi.restoreAllMocks(); });

  it("brings the current START sub-tab into view when the strip scrolls", async () => {
    const original = Element.prototype.scrollIntoView;
    const spy = vi.fn();
    Element.prototype.scrollIntoView = spy;
    try {
      renderApp("/content");
      const tab = await screen.findByTestId("subnav-additional-content");
      await waitFor(() => expect(spy).toHaveBeenCalled());
      expect(tab.getAttribute("aria-current")).toBe("page");
      expect(spy.mock.contexts.some((el) => el === tab)).toBe(true);
      expect(spy).toHaveBeenCalledWith({ inline: "nearest", block: "nearest" });
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("lets the section actions wrap and keeps an end spacer on the sub-tab strip", () => {
    expect(css("pages/content/content.css")).toMatch(/\.v4-csec \.act \{[^}]*flex-wrap: wrap/);
    expect(css("styles/shell.css")).toMatch(/\.tabs\.sub::after \{ content: ""; flex: 0 0 16px; \}/);
  });

  it("keeps the picker source as a flex item that only the name gives way to", () => {
    const block = phoneBlock(css("pages/builder/builder.css"));
    expect(block).toMatch(/\.pick-main, \.pick-list\.with-desc \.pick-main \{ display: flex;/);
    expect(block).toMatch(/\.pick-name \{ flex: 1 1 0; min-width: 0; \}/);
    expect(block).toMatch(/\.pick-src \{ flex: 0 1 auto; min-width: 0; max-width: 55%; \}/);
    expect(block).not.toMatch(/fit-content\(45%\)/);
  });

  // HW1 A3 (emulator, 412 px): START > SOURCES was 500 px wide, so the Source group column was cut at the
  // right edge and two toolbar buttons were off screen. The phone column may shrink below the toolbar,
  // which wraps instead of widening the page. Phones only (html.phone): a narrow desktop window keeps today's
  // layout (U7).
  it("fits START > SOURCES in a phone: the column can shrink, the toolbar wraps and the list keeps a height", () => {
    const block = phoneBlock(css("pages/content/content.css"));
    expect(block).toMatch(/html\.phone \.v4-sources-wrap \{ grid-template-columns: minmax\(0, 1fr\); \}/);
    expect(block).toMatch(/html\.phone \.v4-grid-tools \{[^}]*flex-wrap: wrap/);
    expect(block).toMatch(/html\.phone \.v4-grid-tools \.search \{[^}]*width: 100%/);
    // Landscape (412 px tall): the list kept 0 px of height under the toolbar; it keeps a minimum and the page scrolls.
    expect(block).toMatch(/html\.phone \.v4-grid \{ min-height: \d{3}px; \}/);
  });

  // HW1 A3/Q1: the full-height phone compendium drew its COMPENDIUM title under the Android status bar. On phones the
  // sheet starts below --safe-t (B-48 overlays.css), and on Android --safe-t includes the status bar MainActivity reports.
  it("keeps the phone compendium title out of the status bar", () => {
    const overlays = css("styles/phone/overlays.css");
    expect(overlays).toMatch(/html\.phone \.flyout-overlay\.bottom \{ padding: calc\(var\(--safe-t\) \+ 8px\) 0 0; \}/);
    expect(overlays).toMatch(/html\.phone \.flyout-overlay\.bottom \.flyout \{[^}]*padding-top: 0/);
    expect(css("styles/tokens.css")).toMatch(/:root\.android \{[^}]*--safe-t: max\(env\(safe-area-inset-top, 0px\), var\(--sys-inset-t, 0px\)\)/);
  });
});
