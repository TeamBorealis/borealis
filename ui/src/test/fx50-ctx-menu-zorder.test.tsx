import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// jsdom's getComputedStyle ignores selector specificity, so the tie is checked at source level:
// the real-browser cascade is verified by a separate Playwright check.
const here = dirname(fileURLToPath(import.meta.url));
const contentCss = readFileSync(resolve(here, "../pages/content/content.css"), "utf8");
const shellCss = readFileSync(resolve(here, "../styles/shell.css"), "utf8");

/** Declarations of the first rule whose selector list equals `selector`. */
function declarationsOf(css: string, selector: string): Record<string, string> {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|\\})\\s*${escaped}\\s*\\{([^}]*)\\}`, "m").exec(css);
  if (!match) throw new Error(`rule not found: ${selector}`);
  const out: Record<string, string> = {};
  for (const decl of match[1]!.split(";")) {
    const [prop, ...rest] = decl.split(":");
    if (prop && rest.length) out[prop.trim()] = rest.join(":").trim();
  }
  return out;
}

describe("Sources context menu z-order", () => {
  it("beats the shell .aur-menu-panel tie on specificity with fixed z 80 above the scrim", () => {
    // Arrange
    const shellPanel = declarationsOf(shellCss, ".aur-menu-panel");
    const scrim = declarationsOf(contentCss, ".v4-ctx-scrim");

    // Act
    const menu = declarationsOf(contentCss, ".aur-menu-panel.v4-ctx");

    // Assert
    expect(shellPanel.position).toBe("absolute");
    expect(menu.position).toBe("fixed");
    expect(Number(menu["z-index"])).toBe(80);
    expect(Number(menu["z-index"])).toBeGreaterThan(Number(scrim["z-index"]));
  });
});
