import type { AtRule, Container, Plugin, Root } from "postcss";

/**
 * U9b build step (vite.config.ts `css.postcss`): a phone keeps the phone layout in landscape, where its viewport can be
 * wider than the narrow-shell breakpoint (a Pixel 8 on its side is 915 px). The desktop stylesheets switch to the narrow
 * layout with `@media (max-width: 899px)` and to the wide split with `@media (min-width: 900px)`. So that `html.phone`
 * always gets the narrow layout, whatever its width:
 *
 * - every narrow rule gets a twin right after it, `@media (min-width: 900px) { :where(html.phone) <selector> }`, with the
 *   same declarations, position in the cascade and specificity (`:where()` adds none);
 * - every wide-only rule is scoped to `:where(html:not(.phone))`, which matches exactly as before on desktop.
 *
 * Desktop builds never have `html.phone` (sec. 0), so they render exactly as without this step (U7).
 */
const NARROW = "(max-width: 899px)";
const WIDE = "(min-width: 900px)";
/** The root's class test for each scope. */
const PHONE = ".phone";
const NOT_PHONE = ":not(.phone)";

/** The parts of a media query list (`a, b`); this project's queries have no commas inside parentheses. */
const queries = (params: string): string[] => params.split(",").map((q) => q.trim());

const isNarrow = (params: string): boolean => queries(params).includes(NARROW);
const isWideOnly = (params: string): boolean => {
  const list = queries(params);
  return list.length === 1 && (list[0] === WIDE || list[0]!.startsWith(`${WIDE} and `));
};

/**
 * `sel` limited to documents whose root passes `test`, with no added specificity: a leading `html` / `:root` takes the
 * test in its own compound (`:root:where(.phone)`), any other selector gets `:where(html.phone) ` in front.
 */
export function scopeSelector(sel: string, test: string): string {
  const root = /^(html|:root)(?![\w-])/.exec(sel);
  return root ? `${root[0]}:where(${test})${sel.slice(root[0].length)}` : `:where(html${test}) ${sel}`;
}

/** Scopes every selector of every style rule in `container` (keyframe steps are not selectors). */
function scopeRules(container: Container, test: string): void {
  container.walkRules((rule) => {
    if (rule.parent?.type === "atrule" && /keyframes$/.test((rule.parent as AtRule).name)) return;
    rule.selectors = rule.selectors.map((sel) => scopeSelector(sel, test));
  });
}

export function phoneLandscapeCss(): Plugin {
  return {
    postcssPlugin: "borealis-phone-landscape",
    Once(root: Root) {
      // Collect first, so the twins added below are not visited again.
      const media: AtRule[] = [];
      root.walkAtRules("media", (at) => { media.push(at); });
      for (const at of media) {
        if (isNarrow(at.params)) {
          const twin = at.clone({ params: WIDE, raws: { ...at.raws, before: "\n" } });
          scopeRules(twin, PHONE);
          at.after(twin);
        } else if (isWideOnly(at.params)) {
          scopeRules(at, NOT_PHONE);
        }
      }
    },
  };
}
