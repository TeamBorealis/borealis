import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { createViaShortcut, desktopSuite, expect, NAME, pressKey, settle, SLOW, typeText } from './helpers.js';

// d3: Character Sheet tab -> Generate Preview -> Save Character Sheet (native save panel, pointed at a
// temp dir via Cmd+Shift+G) -> a real PDF: >=2 pages, name, flattened (the default), footer.
desktopSuite('d3', 'Character Sheet: preview and PDF export to a temp dir', async (ctx) => {
  const { app, screen } = ctx;
  // A short lowercase path: the engine types it key by key into the save panel.
  const dir = '/tmp/borealis-e2e-pdf';
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  await app.open();
  await createViaShortcut(ctx, NAME);
  await screen.getByText(/^character sheet$/i).first().tap();
  await screen.getByRole('button', 'Generate Preview').tap();
  await expect(screen.getByText(/No preview yet/)).toBeHidden(SLOW);
  await screen.getByRole('button', 'Save Character Sheet').tap();
  // NSSavePanel: Cmd+Shift+G = go to folder, type the temp dir, Return, Return (Save).
  await settle();
  await pressKey(ctx, 'ControlOrMeta+Shift+g');
  await settle();
  await typeText(ctx, dir);
  await pressKey(ctx, 'Enter');
  await pressKey(ctx, 'Enter');
  await expect.poll(() => readdirSync(dir).filter((f) => f.endsWith('.pdf')).length, SLOW).toBeGreaterThan(0);
  const pdf = readdirSync(dir).find((f) => f.endsWith('.pdf'))!;
  const path = join(dir, pdf);
  expect(statSync(path).size).toBeGreaterThan(1000);
  // Structure (plain-text objects): page count, flattened by default: no AcroForm text widgets; the name is page text.
  const raw = readFileSync(path).toString('latin1');
  expect(raw.startsWith('%PDF-')).toBe(true);
  expect((raw.match(/\/Type\s*\/Page\b/g) ?? []).length).toBeGreaterThanOrEqual(2);
  expect((raw.match(/\/FT\s*\/Tx/g) ?? []).length).toBe(0);
  // Page content streams are compressed (footer lives there): extract text with poppler when present.
  let text = '';
  try {
    text = execFileSync('pdftotext', [path, '-'], { encoding: 'utf8' });
  } catch {
    return;
  }
  expect(text).toContain(NAME);
  expect(text).toContain('Borealis character sheet');
});
