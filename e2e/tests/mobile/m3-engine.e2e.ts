import { expect } from 'e2e';
import { CHARACTER_NAME, characterEntry, createCharacter, expectBuildPage, freshInstall, mobileSuite, navLink, SLOW } from '../../support/mobile.js';

mobileSuite('m3', 'opens the character and shows engine-computed numbers', async ({ app, device, screen }, platform) => {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await createCharacter(screen, device, CHARACTER_NAME);
  await expectBuildPage(screen);
  await app.restart();
  await expect(characterEntry(screen, CHARACTER_NAME)).toBeVisible(SLOW);
  // A double tap occasionally lands as two single taps on iOS: retry until the character is open (Play enables).
  for (let attempt = 0; ; attempt += 1) {
    await characterEntry(screen, CHARACTER_NAME).doubleTap();
    try {
      await expect(navLink(screen, platform, /^Play$/)).toBeVisible({ timeout: 10_000 });
      break;
    } catch (e) {
      if (attempt >= 2) throw e;
    }
  }
  await navLink(screen, platform, /^Play$/).tap();
  await screen.getByLabel(/^Explain Proficiency, \+2$/).tap(); // the value is part of the name now
  // Level 1 proficiency bonus, the Level contribution in the trace: iOS reports the visible "+2"; Android
  // Chromium reports the value span by its aria-label "adds 2" (same engine value, no looser match).
  await expect(screen.getByText(platform === 'ios' ? '+2' : 'adds 2', { exact: true })).toBeVisible(SLOW);
  await expect(screen.getByText('Level 1')).toBeVisible(SLOW);
  await screen.getByRole('button', /^close$/i).first().tap();
  await expect(screen.getByText('Level 1')).toBeHidden(SLOW);
});
