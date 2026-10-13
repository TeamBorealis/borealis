// Usage: node render.mjs <in.svg> <out.png> [size=1024] [background=transparent]
// Needs the `playwright-core` package and a Chromium it can launch, e.g.
//   npm i --no-save playwright-core && npx playwright-core install chromium
import { chromium } from "playwright-core";
import fs from "node:fs";
const [,, svg, out, size="1024", bg="transparent"] = process.argv;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: +size, height: +size } });
const s = fs.readFileSync(svg, "utf8").replace(/width="1024" height="1024"/, `width="${size}" height="${size}"`);
await p.setContent(`<body style="margin:0;background:${bg}">${s}</body>`);
await p.screenshot({ path: out, omitBackground: true });
await b.close();
