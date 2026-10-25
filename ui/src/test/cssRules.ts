import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

/** One style rule: its selector list, the at-rules around it (outermost first) and its declaration text. */
export interface CssRule { selectors: string[]; atRules: string[]; body: string }

/** The `src/` folder of the UI. */
export const SRC = resolve(__dirname, "..");

/** Every .css file under `src/`, relative to it (posix slashes). */
export function cssFiles(dir = SRC): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return cssFiles(path);
    return name.endsWith(".css") ? [relative(SRC, path).split("\\").join("/")] : [];
  });
}

export const readCss = (file: string): string => readFileSync(join(SRC, file), "utf8");

/** Splits a selector list on top-level commas (commas inside :is() / :not() stay). */
export function splitSelectors(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of list) {
    if (ch === "(" || ch === "[") depth++;
    if (ch === ")" || ch === "]") depth--;
    if (ch === "," && depth === 0) { out.push(cur.trim()); cur = ""; } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

/** A small brace walker: enough for this project's CSS (no braces inside strings). Keyframe steps are skipped. */
export function parseRules(css: string): CssRule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: CssRule[] = [];
  const stack: { prelude: string; start: number }[] = [];
  let mark = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === "{") {
      stack.push({ prelude: text.slice(mark, i).trim(), start: i + 1 });
      mark = i + 1;
    } else if (ch === "}") {
      const top = stack.pop();
      if (top && !top.prelude.startsWith("@")) {
        const atRules = stack.map((s) => s.prelude).filter((p) => p.startsWith("@"));
        if (!atRules.some((a) => a.startsWith("@keyframes"))) rules.push({ selectors: splitSelectors(top.prelude), atRules, body: text.slice(top.start, i).trim() });
      }
      mark = i + 1;
    } else if (ch === ";" && (stack.length === 0 || stack[stack.length - 1]!.prelude.startsWith("@"))) {
      mark = i + 1; // a top-level or at-rule statement such as @import
    }
  }
  return rules;
}

/** Custom properties a rule declares (`--name: value`). */
export function declaredVars(body: string): Map<string, string> {
  const vars = new Map<string, string>();
  for (const m of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+)/g)) vars.set(m[1]!, m[2]!.trim());
  return vars;
}
