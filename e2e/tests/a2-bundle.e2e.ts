import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import type { Ctx } from '../support/flows-a.js';

// A2 (device-sync-investigation.md sections 2 and 4, Stage 0): bundles as files against the mock.
// File > Export to bundle... writes a .borealis file; File > Open bundle... (or a drop / OS open) shows the
// same receive preview as a device transfer and Accept applies it. `?mockBundle=<name>` is the file the open
// panel "picks"; names containing `tampered` or `notabundle` are refused with a plain message.
const STATUS_WAIT_MS = 10_000;

const statusText = (ctx: Ctx): Promise<string> =>
  ctx.browser.evaluate((_: null) => (document.querySelector('.aur-status .status-msg') as HTMLElement | null)?.innerText ?? '', null);

async function waitForStatus(ctx: Ctx, pattern: RegExp): Promise<string> {
  const deadline = Date.now() + STATUS_WAIT_MS;
  for (;;) {
    const text = await statusText(ctx);
    if (pattern.test(text)) return text;
    if (Date.now() > deadline) throw new Error(`status bar never matched ${pattern}: "${text}"`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

async function fileMenu(ctx: Ctx, entry: string): Promise<void> {
  await ctx.screen.getByRole('menuitem', 'File').tap();
  await ctx.screen.getByRole('menuitem', entry).tap();
}

describe('a2', { tags: ['a2'] }, () => {
  test('Export to bundle: pick a character, Export..., the status bar names the file', async (ctx) => {
    await bootToHome(ctx, '/#/characters');
    await fileMenu(ctx, 'Export to bundle...');
    await expect(ctx.screen.getByRole('dialog', 'Export to bundle')).toBeVisible();
    await ctx.screen.getByRole('checkbox', 'Seraphine Vale').tap();
    await ctx.screen.getByRole('button', 'Export...').tap();
    await waitForStatus(ctx, /^Exported 1 character to Seraphine Vale\.borealis\.$/i);
  });

  test('Open bundle: the receive preview, Accept applies it, the new character appears', async (ctx) => {
    await bootToHome(ctx, '/#/characters');
    await fileMenu(ctx, 'Open bundle...');
    await expect(ctx.screen.getByRole('dialog', 'Open bundle')).toBeVisible();
    await expect(ctx.screen.getByRole('list', 'What Accept changes')).toBeVisible();
    await expect(ctx.screen.getByText('Replaces your older copy')).toBeVisible();
    await expect(ctx.screen.getByText('Yours is newer: kept')).toBeVisible();
    await ctx.screen.getByRole('button', 'Accept').tap();
    await waitForStatus(ctx, /^Opened Mira\.borealis: 1 new, 1 replaced, 1 kept/i);
    await expect(ctx.screen.getByRole('button', /^Mira, /)).toBeVisible();
  });

  test('a damaged bundle is refused with a plain message and nothing changes', async (ctx) => {
    await bootToHome(ctx, '/?mockBundle=tampered.borealis#/characters');
    await fileMenu(ctx, 'Open bundle...');
    await expect(ctx.screen.getByText(/damaged or was changed after it was made/)).toBeVisible();
    await ctx.screen.getByRole('button', 'OK').tap();
    await expect(ctx.screen.getByRole('dialog', 'Open bundle')).toBeHidden();
  });

  test('phone: More > Share as a file hands the bundle to the share sheet', async (ctx) => {
    await ctx.app.open('/?mock=mobile#/more');
    await ctx.screen.getByRole('button', 'Share as a file').tap();
    await expect(ctx.screen.getByRole('dialog', 'Share as a file')).toBeVisible();
    await ctx.screen.getByRole('checkbox', 'Seraphine Vale').tap();
    await ctx.screen.getByRole('button', 'Share').tap();
    await expect(ctx.screen.getByText(/is ready/)).toBeVisible();
  });
});
