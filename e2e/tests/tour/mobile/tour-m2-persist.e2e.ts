import { expect } from 'e2e';
import { CHARACTER_NAME, characterEntry, createCharacter, expectBuildPage, freshInstall, navLink, SLOW } from '../../../support/mobile.js';
import { openCharacter } from '../../../support/tour-mobile-ui.js';
import { mobileTour, pace } from '../../../support/tour-mobile.js';

// Tour m2: create, restart the app, the character is still listed; open it, Manage, Character Sheet.
// Demo only (skipped unless E2E_TOUR=1); captions are burned into the MP4 afterwards by scripts/collect-videos.mjs.
mobileTour('m2-persist', 'Create, restart the app, the character persists', async ({ app, device, screen }, platform, tour) => {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await tour.caption('Fresh install: no characters yet');
  await pace(1000);

  await tour.caption('Create a character named Mira Test');
  await createCharacter(screen, device, CHARACTER_NAME);
  await expectBuildPage(screen);
  await tour.caption('Character created: the Build page opens');
  await pace(2500);

  await tour.caption('Quit and relaunch the app');
  await app.restart();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await expect(characterEntry(screen, CHARACTER_NAME)).toBeVisible(SLOW);
  await tour.caption('The character is still listed: saved by the Rust backend');
  await pace(3500);

  await tour.caption('Open it: double-tap the character');
  await openCharacter(screen, characterEntry(screen, CHARACTER_NAME), CHARACTER_NAME);
  await pace(2800);

  await tour.caption('More > Manage');
  await navLink(screen, platform, /^More$/).tap();
  await pace(1200);
  await (platform === 'ios' ? screen.getByRole('link', /^Manage$/) : screen.getByLabel(/^Manage$/)).tap();
  await pace(4000);

  await tour.caption('More > Sheet (the character sheet)');
  await navLink(screen, platform, /^More$/).tap();
  await pace(1200);
  await (platform === 'ios' ? screen.getByRole('link', /^Sheet$/) : screen.getByLabel(/^Sheet$/)).tap();
  await pace(4500);
});
