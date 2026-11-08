import { describe, expect, it } from "vitest";
import { parseRules, readCss } from "./cssRules";

// B-231: the Magic page has the same two-pane layout as Equipment (B-61). A long spell description grew the page's grid
// row and stretched the page. In the two-column layout the description pane is taken out of the row's sizing (absolute,
// filling the right column) so the spell lists set the height and the description scrolls inside it, as in the builder.
const rules = parseRules(readCss("pages/spells/magic.css"));
const RIGHT = "html:not(.phone) .ap-page:has(.sp-page) > .ap-right";
const PANE = `${RIGHT} > .desc-pane`;
const ruleFor = (sel: string) => rules.find((r) => r.selectors.includes(sel));
const decls = (body: string) => body.split(";").map((d) => d.trim().replace(/\s+/g, " ")).filter(Boolean);

describe("Magic description pane (B-231)", () => {
  it("fills the right column without sizing the row, in the two-column layout only", () => {
    const right = ruleFor(RIGHT);
    const pane = ruleFor(PANE);
    expect(right && decls(right.body)).toContain("position: relative");
    expect(pane && decls(pane.body)).toEqual(expect.arrayContaining(["position: absolute", "inset: 0"]));
    for (const r of [right!, pane!]) expect(r.atRules).toEqual(["@media (min-width: 900px)"]);
  });

  it("targets the page the spells TwoPane renders (its left column holds .sp-page)", () => {
    expect(readCss("pages/spells/SpellsPage.tsx")).toMatch(/<TwoPane left=\{<div className="sp-page">/);
  });
});
