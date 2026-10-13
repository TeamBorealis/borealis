import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome } from '../support/index.js';

// Importing without AI: content preview then install (XML and .index), and the character import report with a re-check.
// Files go through the hidden file inputs; MockApi runs the whole flow in memory. Deterministic: no model.
const FIXTURES = 'fixtures/import';

async function goTo(ctx: any, hash: string): Promise<void> {
  await ctx.browser.evaluate((h: string) => { location.hash = h; return 1; }, hash);
}
const bodyText = (ctx: any): Promise<string> => ctx.browser.evaluate((_: null) => document.body.innerText, null);

describe('import-flows', { tags: ['import-flows'] }, () => {
  test('content: XML is previewed (counts, unresolved reference) and installed on confirm', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await goTo(ctx, '#/content');
    await expect(screen.getByRole('button', 'Import files')).toBeVisible();
    await browser.locator('input[aria-label="Choose content files"]').setInputFiles(`${FIXTURES}/quick-feats.xml`);
    // The notice comes before anything is read: nothing is previewed until the user continues.
    await expect(screen.getByRole('dialog', /Import only content you own/)).toBeVisible();
    await expect(screen.getByText(/does not condone importing material in breach of copyright/)).toBeVisible();
    await screen.getByRole('button', 'I understand, continue').tap();
    const dlg = screen.getByRole('dialog', /Import preview/);
    await expect(dlg).toBeVisible();
    await expect(screen.getByText(/2 elements from 1 file will be added as "quick-feats"/)).toBeVisible();
    await expect(screen.getByText(/ID_SAMPLE_FEAT_GONE/)).toBeVisible();
    expect(await bodyText(ctx)).not.toMatch(/\bAI\b/);
    await screen.getByRole('button', 'Install').tap();
    await expect(dlg).toBeHidden();
    await expect(screen.getByText('quick-feats').first()).toBeVisible();
  });

  test('content: an .index imports the files it lists and notes the ones that were not chosen', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await goTo(ctx, '#/content');
    await browser.locator('input[aria-label="Choose content files"]').setInputFiles([`${FIXTURES}/sample.index`, `${FIXTURES}/listed.xml`]);
    await screen.getByRole('button', 'I understand, continue').tap();
    const dlg = screen.getByRole('dialog', /Import preview/);
    await expect(dlg).toBeVisible();
    await expect(screen.getByText(/1 element from 2 files will be added/)).toBeVisible();
    await expect(screen.getByRole('button', /Notes \(1\)/)).toBeVisible();
    await screen.getByRole('button', 'Install').tap();
    await expect(dlg).toBeHidden();
  });

  test('characters: the report lists missing content; installing it and re-checking clears it', async (ctx) => {
    const { screen, browser } = ctx;
    await bootToHome(ctx);
    await browser.locator('input[aria-label="Import character files"]').setInputFiles([`${FIXTURES}/fine.dnd5e`, `${FIXTURES}/needs-missing.dnd5e`]);
    const dlg = screen.getByRole('dialog');
    await expect(dlg).toBeVisible();
    await expect(screen.getByText(/2 of 2 characters imported, some with missing content/)).toBeVisible();
    await expect(screen.getByText(/ID_WOTC_TCOE_FEAT_SKILL_EXPERT/)).toBeVisible();
    await expect(screen.getByText(/ID_HOMEBREW_SPELL_RADIANT_BLAST/)).toBeVisible();
    await screen.getByRole('button', 'Done').tap();
    await expect(dlg).toBeHidden();
    await expect(screen.getByRole('button', /Re-check missing content/)).toBeVisible();

    // Install the content the character needed (any pack), come back and re-check.
    await goTo(ctx, '#/content');
    await browser.locator('input[aria-label="Choose content files"]').setInputFiles(`${FIXTURES}/quick-feats.xml`);
    await screen.getByRole('button', 'I understand, continue').tap();
    await screen.getByRole('button', 'Install').tap();
    await expect(screen.getByRole('dialog', /Import preview/)).toBeHidden();
    await goTo(ctx, '#/characters');
    await screen.getByRole('button', /Re-check missing content/).tap();
    await expect(screen.getByRole('button', /Re-check missing content/)).toBeHidden();
  });
});
