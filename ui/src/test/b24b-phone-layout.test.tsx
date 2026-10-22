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
});
