import { describe, expect, it } from "vitest";
import { cssFiles, parseRules, readCss } from "./cssRules";

// B-48 WP1 (mobile-facelift-spec sec. 5.8): WebKit and Chromium keep :hover applied after a tap, so a hover style sticks on
// phones. Every :hover rule sits inside a hover-capable media query; with a mouse (desktop) nothing changes.
const HOVER_MEDIA = "@media (hover: hover) and (pointer: fine)";

describe("hover guard (B-48 WP1)", () => {
  it("finds the project's CSS", () => {
    expect(cssFiles().length).toBeGreaterThan(20);
  });

  it("no :hover rule applies outside a hover-capable pointer", () => {
    const bare = cssFiles().flatMap((file) => parseRules(readCss(file))
      .filter((r) => r.selectors.some((s) => s.includes(":hover")) && !r.atRules.includes(HOVER_MEDIA))
      .map((r) => `${file}: ${r.selectors.join(", ")}`));
    expect(bare).toEqual([]);
  });

  it("hover rules hold only hover selectors (states such as :focus-visible or [aria-expanded] stay unguarded)", () => {
    const mixed = cssFiles().flatMap((file) => parseRules(readCss(file))
      .filter((r) => r.atRules.includes(HOVER_MEDIA) && r.selectors.some((s) => !s.includes(":hover")))
      .map((r) => `${file}: ${r.selectors.join(", ")}`));
    expect(mixed).toEqual([]);
  });
});
