import { describe, expect, it } from "vitest";
import { parseRules, readCss } from "./cssRules";

// B-61: on the Equipment page a long item description grew the page's grid row, pushing the tools bar and the inventory off
// screen. In the two-column layout the description pane is taken out of the row's sizing (absolute, filling the right
// column) so the catalogue and inventory set the height and the description scrolls inside it, as in the builder.
const rules = parseRules(readCss("pages/equipment/equipment.css"));
const RIGHT = "html:not(.phone) .ap-page:has(.eq-layout) > .ap-right";
const PANE = `${RIGHT} > .desc-pane`;
const ruleFor = (sel: string) => rules.find((r) => r.selectors.includes(sel));
const decls = (body: string) => body.split(";").map((d) => d.trim().replace(/\s+/g, " ")).filter(Boolean);

describe("Equipment description pane (B-61)", () => {
  it("fills the right column without sizing the row, in the two-column layout only", () => {
    const right = ruleFor(RIGHT);
    const pane = ruleFor(PANE);
    expect(right && decls(right.body)).toContain("position: relative");
    expect(pane && decls(pane.body)).toEqual(expect.arrayContaining(["position: absolute", "inset: 0"]));
    for (const r of [right!, pane!]) expect(r.atRules).toEqual(["@media (min-width: 900px)"]);
  });

  it("scrolls the description inside the pane (the shared description pane styles)", () => {
    const builder = parseRules(readCss("pages/builder/builder.css"));
    const pane = builder.find((r) => r.selectors.includes(".desc-pane") && r.atRules.length === 0)!;
    const scroll = builder.find((r) => r.selectors.includes(".desc-scroll") && r.atRules.length === 0)!;
    expect(decls(pane.body)).toEqual(expect.arrayContaining(["display: flex", "flex-direction: column", "min-height: 0"]));
    expect(decls(scroll.body)).toEqual(expect.arrayContaining(["flex: 1", "min-height: 0", "overflow-y: auto"]));
    const spells = parseRules(readCss("pages/spells/pane.css"));
    expect(spells.some((r) => r.selectors.includes(".ap-right .desc-scroll") && /overflow-y:\s*scroll/.test(r.body))).toBe(true);
  });
});
