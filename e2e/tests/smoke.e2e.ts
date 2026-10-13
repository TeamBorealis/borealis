import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, realClick } from '../support/index.js';

// Deterministic only: passes with no model configured.
describe('smoke', { tags: ['smoke'] }, () => {
  test('boots to the shell and opens the New character flyout', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);

    // main shell: menu bar + the seeded characters on the start screen
    await expect(screen.getByRole('menuitem', 'View')).toBeVisible();
    await expect(screen.getByRole('button', 'Seraphine Vale, Level 2 Elf Wizard')).toBeVisible();

    // Trigger New Character with a real pointer (z-order hit test first); flyout opens.
    await realClick(ctx, browser.locator('button.aur-newchar'), 'button.aur-newchar');
    await expect(screen.getByRole('dialog', 'New character')).toBeVisible();
    await expect(screen.getByRole('button', 'Create character')).toBeVisible();


    // Cancel changes nothing.
    await screen.getByRole('button', 'Cancel').tap();
    await expect(screen.getByRole('dialog', 'New character')).toBeHidden();
  });
});
