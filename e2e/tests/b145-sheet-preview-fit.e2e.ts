import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bootToHome } from '../support/index.js';

// B-145 (tester eylonke, 2026-11-02): in a phone-wide preview page 1 was laid out narrower than its paper, so the HP,
// hit dice and death saves boxes overlapped, Size read "Mediu" and Speed wrapped to "30 / ft". This loads real renderer
// output (2024 on A4, 2014 on Letter, both with the vitals at their widest) through the app's own `framePreviewHtml`
// into frames from phone to desktop width, and checks in Chromium that every page keeps its paper shape, nothing on
// page 1 leaves its box or the page, and no strip value wraps.
// B-146 (same tester): 2014 page 1 and the spellcasting page ran long. The preview now splits them as the PDF does, so
// every page of every sample (incl. the 2014 cleric on both papers, whose spells run over two pages) is as tall as its
// paper, and nothing on a page runs past its content box into the footer.
// Fixtures: `BOREALIS_HTML_SAMPLE=<dir> cargo test -p borealis-sheet --lib html_sheet_sample`, then copy
// `2024-a4-long.html`, `2014-letter-long.html`, `2014-letter.html` and `2014-a4.html` from <dir> to
// e2e/fixtures/sheet-preview/.
const here = dirname(fileURLToPath(import.meta.url));
const FIXTURES = [
  { file: '2024-a4-long.html', ratio: 842 / 595 },
  { file: '2014-letter-long.html', ratio: 792 / 612 },
  { file: '2014-letter.html', ratio: 792 / 612 },
  { file: '2014-a4.html', ratio: 842 / 595 },
];
/** Phone (360, 376 = a 392 px phone's frame), iPad portrait pane (440), tablet (744) and desktop widths. */
const WIDTHS = [360, 376, 440, 744, 1024];

type Probe = { html: string; width: number };
type Found = { ratios: number[]; heights: number[]; problems: string[] };

/** Runs in the page: frame the document with the app's preview CSS and measure page 1. */
async function probe({ html, width }: Probe): Promise<Found> {
  const url = '/src/pages/sheet/previewFrame.ts';
  const mod = (await import(/* @vite-ignore */ url)) as { framePreviewHtml: (h: string) => string };
  document.getElementById('b145-frame')?.remove();
  const frame = document.createElement('iframe');
  frame.id = 'b145-frame';
  frame.style.cssText = `position:fixed;left:0;top:0;width:${width}px;height:900px;border:0;z-index:99999`;
  await new Promise<void>((ok) => { frame.onload = () => ok(); frame.srcdoc = mod.framePreviewHtml(html); document.body.append(frame); });
  const doc = frame.contentDocument as Document;
  const problems: string[] = [];
  const pages = [...doc.querySelectorAll('.page')];
  const ratios = pages.map((p) => { const b = p.getBoundingClientRect(); return b.height / b.width; });
  const name = (el: Element) => `${el.className || el.tagName} "${(el.textContent ?? '').trim().slice(0, 24)}"`;
  const heights = pages.map((p) => p.getBoundingClientRect().height);
  // B-146: a page's content never runs into its footer (the page box is fixed and would hide it).
  pages.forEach((p, i) => {
    const limit = (p.querySelector('.foot') as Element).getBoundingClientRect().top;
    for (const el of p.querySelectorAll('.content *')) {
      if (el.closest('svg')) continue;
      const b = el.getBoundingClientRect();
      if (b.height && b.bottom > limit + 0.5) { problems.push(`page ${i + 1}: ${name(el)} runs into the footer`); break; }
    }
  });
  const p1 = pages[0].getBoundingClientRect();
  if (p1.right > doc.documentElement.clientWidth + 0.5) problems.push(`page 1 is wider than the ${width} px frame`);
  for (const el of pages[0].querySelectorAll('.head *, .vitals *, .strip *')) {
    const b = el.getBoundingClientRect();
    if (!b.width) continue;
    if (b.right > p1.right + 0.5) problems.push(`${name(el)} passes the page edge`);
    const box = el.parentElement?.closest('.box');
    if (!box) continue;
    const bb = box.getBoundingClientRect();
    if (b.left < bb.left - 0.5 || b.right > bb.right + 0.5 || b.bottom > bb.bottom + 0.5) problems.push(`${name(el)} leaves its box`);
  }
  const boxes = [...pages[0].querySelectorAll('.vitals > .box')].map((b) => b.getBoundingClientRect());
  boxes.forEach((a, i) => boxes.slice(i + 1).forEach((c, k) => {
    if (a.left < c.right - 0.5 && c.left < a.right - 0.5 && a.top < c.bottom - 0.5 && c.top < a.bottom - 0.5) problems.push(`vitals box ${i} overlaps box ${i + k + 1}`);
  }));
  for (const el of pages[0].querySelectorAll('.strip .mid, .vitals .big')) {
    const range = doc.createRange();
    range.selectNodeContents(el);
    const lines = new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
    if (lines > 1) problems.push(`${name(el)} wraps onto ${lines} lines`);
  }
  frame.remove();
  return { ratios, heights, problems };
}

describe('B-145 sheet preview fit', { tags: ['b145'] }, () => {
  test('every page keeps its paper shape and height, and page 1 vitals never overlap or leave the page, phone to desktop', async (ctx) => {
    await bootToHome(ctx);
    for (const { file, ratio } of FIXTURES) {
      const html = readFileSync(join(here, '../fixtures/sheet-preview', file), 'utf8');
      for (const width of WIDTHS) {
        const found = (await ctx.browser.evaluate(probe, { html, width })) as Found;
        const at = `${file} at ${width} px`;
        expect({ at, problems: found.problems }).toEqual({ at, problems: [] });
        for (const r of found.ratios) expect({ at, off: Math.abs(r - ratio) < 0.01 }).toEqual({ at, off: true });
        expect({ at, heights: new Set(found.heights.map(Math.round)).size }).toEqual({ at, heights: 1 });
      }
    }
  });
});
