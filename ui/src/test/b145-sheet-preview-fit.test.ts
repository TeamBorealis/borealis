import { describe, expect, it } from "vitest";
import { framePreviewHtml, PREVIEW_CSS } from "../pages/sheet/previewFrame";
import { parseRules } from "./cssRules";

// B-145 (tester eylonke, 2026-11-02): in a phone-wide preview the sheet page was laid out narrower than its paper, so the
// HP, hit dice and death saves boxes overlapped and Size and Speed were cut. The preview must keep the page at its
// printed width (612 pt Letter, 595 pt A4) and scale the whole page to the frame instead.

const LETTER = '<html><head></head><body class="sheet"><div class="desk"><section class="page letter" data-page="character"></section></div></body></html>';
const A4 = LETTER.replace("page letter", "page a4");
/** Printed page widths in CSS px (1 pt = 4/3 px). */
const PAGE_PX = { letter: 612 * (4 / 3), a4: 595 * (4 / 3) };
/** The classic scrollbar the preview reserves. */
const SCROLLBAR = 17;
const WIDTHS = [320, 360, 376, 392, 440, 600, 744, 1024, 1600];

const styleOf = (html: string): string => html.slice(html.indexOf("<style>") + 7, html.indexOf("</style>"));

/** The `.desk` zoom the frame's CSS gives at a viewport `width` (the last matching rule wins). */
function zoomAt(css: string, width: number): number {
  let zoom = 1;
  for (const r of parseRules(css)) {
    if (!r.selectors.some((s) => /\.desk$/.test(s))) continue;
    const m = /zoom:\s*([\d.]+)/.exec(r.body);
    if (!m) continue;
    const min = r.atRules.map((a) => /min-width:\s*(\d+)px/.exec(a)).find(Boolean);
    const max = r.atRules.map((a) => /max-width:\s*(\d+)px/.exec(a)).find(Boolean);
    if (min && width < Number(min[1])) continue;
    if (max && width > Number(max[1])) continue;
    zoom = Number(m[1]);
  }
  return zoom;
}

describe("B-145 sheet preview keeps the printed page and scales it", () => {
  it("never overrides a page's width or height", () => {
    for (const r of parseRules(PREVIEW_CSS)) {
      if (!r.selectors.some((s) => /\.(page|letter|a4)\b/.test(s))) continue;
      expect(r.body, r.selectors.join(",")).not.toMatch(/(^|;)\s*(width|max-width|min-height|height)\s*:/);
    }
  });

  it.each(WIDTHS)("fits a Letter page into a %i px frame without reflowing it", (width) => {
    const zoom = zoomAt(styleOf(framePreviewHtml(LETTER)), width);
    const drawn = PAGE_PX.letter * zoom;
    expect(drawn).toBeLessThanOrEqual(width - SCROLLBAR);
    // and uses the frame: at most one 10 px step narrower than the room it has
    expect(drawn).toBeGreaterThan(width - SCROLLBAR - 12);
  });

  it.each(WIDTHS)("fits an A4 page into a %i px frame by its own width", (width) => {
    const zoom = zoomAt(styleOf(framePreviewHtml(A4)), width);
    const drawn = PAGE_PX.a4 * zoom;
    expect(drawn).toBeLessThanOrEqual(width - SCROLLBAR);
    expect(drawn).toBeGreaterThan(width - SCROLLBAR - 12);
  });

  it("overrides the renderer's fixed phone zoom", () => {
    // The renderer's own `@media(max-width:700px){.desk{zoom:.55}}` must lose to the preview's rule.
    const css = styleOf(framePreviewHtml(LETTER));
    expect(parseRules(css).filter((r) => r.selectors.includes("body.sheet .desk")).length).toBeGreaterThan(10);
  });
});
