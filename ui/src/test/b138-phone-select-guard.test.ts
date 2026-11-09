import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

// U22 (B-138, tester C2): no WebView select dialog on phones. Every select in the app goes through ComboBox or SelectField,
// which render the native select only on desktop and the select sheet (SelectSheet) on phones. A raw `<select>`,
// `<optgroup>` or `<option>` anywhere else would bring the native dialog back on phones, so this guard fails on one. A
// `<datalist>`'s options are a text field's suggestions, not a select, and are allowed.

const SRC = resolve(__dirname, "..");

/** The two wrappers that may render the native select, each only on desktop (`usePhone()` false). */
const ALLOWED = new Set(["components/controls/Controls.tsx", "components/Fields.tsx"]);
const RAW_SELECT = /<(select|optgroup|option)\b|createElement\(\s*["'](select|optgroup|option)["']/;
/** A `<datalist>` (a text field's suggestions, no select dialog) may hold options. */
const DATALIST = /<datalist\b[\s\S]*?<\/datalist>/g;

function sourceFiles(dir = SRC): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "test" ? [] : sourceFiles(path);
    if (!/\.(tsx?|jsx?)$/.test(name) || /\.test\.(tsx?|jsx?)$/.test(name)) return [];
    return [relative(SRC, path).split("\\").join("/")];
  });
}

describe("B-138 phone selects", () => {
  it("no source file outside ComboBox and SelectField renders a raw select, optgroup or option", () => {
    // Arrange
    const files = sourceFiles();

    // Act
    const offenders = files.filter((f) => !ALLOWED.has(f) && RAW_SELECT.test(readFileSync(join(SRC, f), "utf8").replace(DATALIST, "")));

    // Assert
    expect(files.length).toBeGreaterThan(100);
    expect(offenders).toEqual([]);
  });

  it("ComboBox and SelectField render the select sheet on phones and the native select only otherwise", () => {
    for (const file of ALLOWED) {
      // Arrange
      const text = readFileSync(join(SRC, file), "utf8");

      // Act
      const phoneBranch = /const phone = usePhone\(\);/.test(text) && /phone\s*\?\s*<SelectSheet\b/.test(text.replace(/\s+/g, " "));

      // Assert
      expect(phoneBranch, file).toBe(true);
    }
  });
});
