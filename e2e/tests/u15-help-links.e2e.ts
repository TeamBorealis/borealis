import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';
import { waitFor } from '../support/flows-a.js';

// GitHub #9 (decision U15): Help > News / Releases / Support Page and the title-bar globe open the app's own web pages
// through the backend `open_link` command (the mock host records each address on window.__mockOpenedLinks). Support
// Borealis is hidden; nothing says the website is "not published yet". Deterministic: no model.
const WANT = [
  'https://borealisbuilder.com/changelog',
  'https://github.com/TeamBorealis/borealis/releases',
  'https://discord.gg/r4K4D3AJpZ',
  'https://borealisbuilder.com/',
];

describe('#9 Help menu links', { tags: ['u15'] }, () => {
  test('News, Releases, Support Page and the globe open their pages; Support Borealis is hidden', async (ctx) => {
    const { screen } = ctx;
    await bootToHome(ctx);
    for (const entry of ['News', 'Releases', 'Support Page']) {
      await screen.getByRole('menuitem', 'Help').tap();
      await screen.getByRole('menuitem', entry).tap();
    }
    await screen.getByRole('menuitem', 'Help').tap();
    await expect(screen.getByRole('menuitem', 'Get Started')).toBeVisible();
    await expect(screen.getByRole('menuitem', 'Support Borealis')).toBeHidden();
    await screen.getByRole('menuitem', 'Help').tap();
    await screen.getByRole('button', 'Project website').tap();
    await waitFor(ctx, (n: number) => ((globalThis as { __mockOpenedLinks?: string[] }).__mockOpenedLinks ?? []).length >= n, WANT.length, 'four opened links');
    expect(await ctx.browser.evaluate((_: null) => (globalThis as { __mockOpenedLinks?: string[] }).__mockOpenedLinks ?? [], null)).toEqual(WANT);
    const status = await ctx.browser.evaluate((_: null) => document.body.innerText, null);
    expect(/not published yet/i.test(status)).toBe(false);
  });
});
