import { expect } from 'e2e';
import { CHARACTER_NAME, characterEntry, createCharacter, expectBuildPage, expectStartPage, freshInstall, mobileSuite, SLOW } from '../../support/mobile.js';

// m2: create, relaunch the process, and the character is still listed (Rust backend persistence).
mobileSuite('m2', 'character survives a terminate and relaunch', async ({ app, device, screen }, platform) => {
  await freshInstall(device, platform);
  await app.open();
  await expectStartPage(screen);
  await createCharacter(screen, device, CHARACTER_NAME);
  await expectBuildPage(screen);
  await app.restart();
  await expectStartPage(screen);
  await expect(characterEntry(screen, CHARACTER_NAME)).toBeVisible(SLOW);
});
