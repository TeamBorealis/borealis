// Renders art-kit previews (PNG) for both sets with headless Chromium (playwright-core from app/e2e).
//   node tools/sheet-art/preview.mjs <out-dir>
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { PIECES, SETS, svgOf } from "./build.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(process.argv[2] ?? "art-preview");
mkdirSync(outDir, { recursive: true });
const { chromium } = createRequire(join(here, "../../e2e/package.json"))("playwright-core");
const by = Object.fromEntries(PIECES.map((p) => [p[1], p]));
const inline = (set, name, style = "") => `<div class="a" style="${style}">${svgOf(set, by[name])}</div>`;
const mirror = (set, sx, sy, pos) => `<div class="a" style="position:absolute;${pos};transform:scale(${sx},${sy})">${svgOf(set, by.corner)}</div>`;

const kit = (set) => `<!doctype html><meta charset=utf-8><style>
body{margin:0;background:#FBF9F4;font:11px sans-serif;color:#555;width:1200px}.row{display:flex;gap:18px;padding:14px 24px;align-items:center}
.sig svg{width:110px;height:110px}.a svg{display:block}.cap{font-size:10px;margin:4px 0}.glyph svg{width:48px;height:48px}.rib svg{width:1100px;height:200px}
.page{position:relative;width:612px;height:792px;background:#FBF9F4;box-shadow:0 0 0 1px #ccc;margin:20px 24px}.page>.a{position:absolute;left:0;top:0}
</style><body>
<div class="row sig">${["str", "dex", "con", "int", "wis", "cha"].map((k) => inline(set, `sigil-${k}`)).join("")}
 <div class="glyph">${inline(set, "hit-die")}</div><div class="glyph">${inline(set, "death-save")}</div></div>
<div class="row" style="align-items:flex-start"><div><div class="a" style="transform:scale(2);transform-origin:0 0;width:248px;height:248px">${svgOf(set, by.corner)}</div></div>
 <div style="margin-left:140px"><div class="cap">divider x2</div><div class="a" style="transform:scale(1.6);transform-origin:0 0;width:640px;height:32px">${svgOf(set, by.divider)}</div>
 <div class="cap" style="margin-top:20px">header rule</div>${inline(set, "header-rule")}</div></div>
<div class="row rib">${inline(set, "borealis-ribbon")}</div>
<div class="row"><div class="page">${inline(set, "frame-letter")}${mirror(set, 1, 1, "left:8px;top:8px")}${mirror(set, -1, 1, "right:8px;top:8px")}${mirror(set, 1, -1, "left:8px;bottom:8px")}${mirror(set, -1, -1, "right:8px;bottom:8px")}
<div style="position:absolute;left:46px;top:46px;width:520px;height:90px;opacity:0.8" class="a">${svgOf(set, by["borealis-ribbon"]).replace("<svg ", '<svg preserveAspectRatio="none" style="width:520px;height:90px" ')}</div>
<div style="position:absolute;left:106px;top:150px;width:400px" class="a">${svgOf(set, by.divider)}</div></div></div></body>`;

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 2 });
  for (const set of Object.keys(SETS)) {
    await page.setContent(kit(set));
    await page.screenshot({ path: join(outDir, `art-kit-${set.toLowerCase()}.png`), fullPage: true });
    // close-up of the six sigils at print scale (40 pt) next to 3x
    await page.setContent(`<body style="margin:0;background:#FBF9F4;display:flex;gap:10px;padding:16px">${["str", "dex", "con", "int", "wis", "cha"].map((k) => `<div style="width:230px;height:230px">${svgOf(set, by[`sigil-${k}`]).replace("<svg ", '<svg style="width:230px;height:230px" ')}</div>`).join("")}</body>`);
    await page.setViewportSize({ width: 1500, height: 270 });
    await page.screenshot({ path: join(outDir, `sigils-${set.toLowerCase()}.png`) });
    await page.setViewportSize({ width: 1200, height: 900 });
  }
  process.stdout.write(`previews in ${outDir}\n`);
} finally { await browser.close(); }
