import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { clickByText, clickCss, loadSeraphine, openLevelUp, textOf, waitFor } from '../support/flows-a.js';

// B2a / #34: a character's levels divide between classes without levelling up one at a time.
// Seeded character: Seraphine Vale, level 2 Elf Wizard (MockApi). Deterministic: no model.
const FLYOUT = '.flyout';
const flyoutText = (ctx: Parameters<typeof textOf>[0]) => textOf(ctx, FLYOUT).then((t) => t.replace(/\n+/g, ' / '));
const lastDialogText = (ctx: Parameters<typeof textOf>[0]): Promise<string> =>
  ctx.browser.evaluate((_: null) => (Array.from(document.querySelectorAll('[role=dialog]')).pop() as HTMLElement | undefined)?.innerText ?? '', null);

describe('b228 multiclass split', { tags: ['b228'] }, () => {
  test('MOVE LEVELS splits Wizard 2 into Wizard 1 / Fighter 1, and Remove gives the level back', async (ctx) => {
    const { screen } = ctx;
    await loadSeraphine(ctx);
    await openLevelUp(ctx);
    await clickCss(ctx, 'button[aria-label="Move levels to Fighter"]');
    await expect(screen.getByRole('dialog', 'Move levels to Fighter')).toBeVisible();
    const dialog = await lastDialogText(ctx);
    expect(dialog).toContain('Your character stays level 2.');
    expect(dialog.toUpperCase()).toContain('WIZARD 1 / FIGHTER 1');
    expect(dialog).toContain('Choices made at Wizard level 2 are dropped.');
    await clickByText(ctx, '.dialog-foot button', 'Move levels');
    await waitFor(ctx, (_: null) => /FIGHTER \(1\)/i.test(document.querySelector('.flyout')?.textContent ?? ''), null, 'Fighter card');
    let text = await flyoutText(ctx);
    expect(text).toContain('YOUR CHARACTER IS LEVEL 2');
    expect(text).toContain('WIZARD (1)');

    await clickCss(ctx, 'button[aria-label="Remove multiclass Fighter"]');
    await expect(screen.getByRole('dialog', 'Remove multiclass')).toBeVisible();
    expect(await lastDialogText(ctx)).toContain('Its level goes back to Wizard (Wizard 2)');
    await clickByText(ctx, '.dialog-foot button', 'Remove multiclass');
    await waitFor(ctx, (_: null) => !/FIGHTER \(1\)/i.test(document.querySelector('.flyout')?.textContent ?? ''), null, 'Fighter card gone');
    text = await flyoutText(ctx);
    expect(text).toContain('WIZARD (2)');
    expect(text).toContain('YOUR CHARACTER IS LEVEL 2');
  });
});
