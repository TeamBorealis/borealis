import { expect } from 'e2e';
import { CHARACTER_NAME, createCharacter, expectBuildPage, expectStartPage, freshInstall, mobileSuite, navLink, SLOW, waitForShell } from '../../support/mobile.js';

// m9 (U22, B-138, tester C2): on a phone the Gear category select opens the app's bottom sheet, not the WebView's select
// dialog (which scrolled under a drifting finger and picked Trade Goods for Equipment Packs). Choosing "Equipment Packs"
// in the sheet shows the packs. Phones only: a tablet's wide shell keeps the native select.
mobileSuite('m9', 'chooses Equipment Packs from the Gear category sheet', async ({ app, device, screen }, platform) => {
  await freshInstall(device, platform);
  await app.open();
  await expectStartPage(screen);
  const shell = await waitForShell(screen);
  if (shell !== 'phone') return;
  await createCharacter(screen, device, CHARACTER_NAME);
  await expectBuildPage(screen);
  await navLink(screen, platform, /^Gear$/).tap();
  await screen.getByText(/^Equipment$/).first().tap();
  const category = screen.getByRole('button', /^Category$/);
  await expect(category).toBeVisible(SLOW);
  await category.tap();
  // The sheet: titled by the select, a Close button, every category as a row (one of them ticked as current).
  const packs = screen.getByText(/^Equipment Packs \(\d+\)$/);
  await expect(packs).toBeVisible(SLOW);
  await expect(screen.getByText(/^Trade Goods \(\d+\)$/)).toBeVisible();
  await expect(screen.getByRole('button', /^Close$/).first()).toBeVisible();
  await packs.tap();
  // The sheet closes and the catalogue shows the packs under the "Equipment Packs" band. (The select button's label is
  // its name, "Category"; the chosen value is its description, which the Android tree does not list as text.)
  await expect(screen.getByRole('button', /^Close$/)).toBeHidden(SLOW);
  await expect(screen.getByText(/^Trade Goods \(\d+\)$/)).toBeHidden();
  await expect(screen.getByText(/^Equipment Packs$/).first()).toBeVisible(SLOW);
  await expect(screen.getByRole('button', /^Explorer.s Pack$/)).toBeVisible(SLOW);
  await expect(screen.getByRole('button', /^Burglar.s Pack$/)).toBeVisible();
});
