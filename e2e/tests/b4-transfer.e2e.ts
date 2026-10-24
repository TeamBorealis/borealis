import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import type { Ctx } from '../support/flows-a.js';

// B4 / C1 (device-sync-investigation.md section 4): Send to device / Receive from device against the mock,
// with a simulated other device (`?mockPeer=<ms>` answers an open window; `?mockScan=1` pretends the phone
// camera read the other device's QR). Both QR directions, the typed code, and Settings > Devices > Remove.
const PEER_MS = 1200;
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

describe('b4', { tags: ['b4'] }, () => {
  test('PC receives: the QR window, the phone sends, preview, Accept, status bar, new tile', async (ctx) => {
    await bootToHome(ctx, `/?mockPeer=${PEER_MS}#/characters`);
    await fileMenu(ctx, 'Receive from device...');
    const dlg = ctx.screen.getByRole('dialog', 'Receive from device');
    await expect(dlg).toBeVisible();
    await expect(ctx.screen.getByRole('img', 'Connection QR code')).toBeVisible();
    await expect(ctx.screen.getByText('42-ember-lantern')).toBeVisible();
    await expect(ctx.screen.getByRole('list', 'What Accept changes')).toBeVisible();
    await ctx.screen.getByRole('button', 'Accept').tap();
    await waitForStatus(ctx, /^Received from Borealis on iPhone or iPad: 1 new, 1 replaced, 1 kept/i);
    await expect(ctx.screen.getByRole('button', /^Mira, /)).toBeVisible();
  });

  test('PC sends: tick a character, Show code, the phone scans and accepts', async (ctx) => {
    await bootToHome(ctx, `/?mockPeer=${PEER_MS}#/characters`);
    await fileMenu(ctx, 'Send to device...');
    await expect(ctx.screen.getByRole('dialog', 'Send to device')).toBeVisible();
    await ctx.screen.getByRole('checkbox', 'Seraphine Vale').tap();
    await ctx.screen.getByRole('button', 'Show code').tap();
    await expect(ctx.screen.getByRole('img', 'Connection QR code')).toBeVisible();
    await expect(ctx.screen.getByText(/Waiting for Borealis on iPhone or iPad to accept/)).toBeVisible();
    await waitForStatus(ctx, /^Sent to Borealis on iPhone or iPad: /i);
  });

  test('PC to PC: the sender types the receiving computer\'s code', async (ctx) => {
    await bootToHome(ctx, `/?mockPeer=${PEER_MS}#/characters`);
    await fileMenu(ctx, 'Send to device...');
    await ctx.screen.getByRole('checkbox', 'Seraphine Vale').tap();
    await ctx.screen.getByRole('button', 'Enter a code').tap();
    await ctx.screen.getByLabel('CODE').fill('42-ember-lantern');
    await ctx.screen.getByRole('button', 'Connect').tap();
    await waitForStatus(ctx, /^Sent to /i);
  });

  test('phone receives by scanning the PC\'s Send QR', async (ctx) => {
    await ctx.app.open(`/?mock=mobile&mockScan=1#/more`);
    await ctx.screen.getByRole('button', 'Receive from device').tap();
    await ctx.screen.getByRole('button', 'Scan the code on the other device').tap();
    await expect(ctx.screen.getByRole('list', 'What Accept changes')).toBeVisible();
    await ctx.screen.getByRole('button', 'Accept').tap();
    await waitForStatus(ctx, /^Received from Borealis on Mac: /i);
  });

  test('phone sends by scanning the PC\'s Receive QR; Settings > Devices lists the PC and Remove forgets it', async (ctx) => {
    await ctx.app.open(`/?mock=mobile&mockScan=1&mockPeer=${PEER_MS}#/more`);
    await ctx.screen.getByRole('button', 'Send to device').tap();
    await ctx.screen.getByRole('checkbox', 'Seraphine Vale').tap();
    await ctx.screen.getByRole('button', 'Next').tap();
    await ctx.screen.getByRole('button', 'Scan the code on the other device').tap();
    await waitForStatus(ctx, /^Sent to Borealis on Mac: /i);

    await ctx.screen.getByRole('menuitem', 'View').tap();
    await ctx.screen.getByRole('menuitem', 'Borealis Settings').tap();
    await ctx.screen.getByRole('tab', 'Devices').tap();
    await expect(ctx.screen.getByRole('group', 'Borealis on Mac')).toBeVisible();
    await ctx.screen.getByRole('button', 'Remove').tap();
    await ctx.screen.getByRole('dialog', 'Remove device').getByRole('button', 'Remove').tap();
    await waitForStatus(ctx, /^Removed 'Borealis on Mac' from your devices\.$/i);
    await expect(ctx.screen.getByText(/No devices yet/)).toBeVisible();
  });
});
