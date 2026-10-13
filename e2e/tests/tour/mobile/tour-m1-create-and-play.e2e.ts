import { expect } from 'e2e';
import { CHARACTER_NAME, expectBuildPage, freshInstall, navLink, SLOW } from '../../../support/mobile.js';
import { buildTab, pickRow } from '../../../support/tour-mobile-ui.js';
import { mobileTour, pace } from '../../../support/tour-mobile.js';

// Tour m1: fresh install -> create a character -> species, class, background -> Play -> Explain Proficiency.
// Demo only (skipped unless E2E_TOUR=1); captions are burned into the MP4 afterwards by scripts/collect-videos.mjs.
mobileTour('m1-create-and-play', 'Create a character, walk Build, open Play', async ({ app, device, screen }, platform, tour) => {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  await tour.caption('Fresh install: the Start screen');
  await pace(1200);

  await tour.caption('Create a new character');
  await screen.getByRole('button', /CREATE A NEW CHARACTER/).tap();
  const field = screen.getByRole('textbox');
  await expect(field).toBeVisible({ timeout: 15_000 });
  await pace(1200);
  await tour.caption('Type a name');
  await field.fill(CHARACTER_NAME);
  await pace(1200);
  await device.dismissKeyboard();
  await tour.caption('Create character');
  await screen.getByRole('button', 'CREATE CHARACTER').tap();
  await expectBuildPage(screen);
  await pace(1500);

  await tour.caption('Build > Species: double-tap Elf');
  await pickRow(screen, platform, /^Elf, SRD 5\.2\.1/).doubleTap();
  await pace(1800);

  await tour.caption('Elven lineage: double-tap High Elf');
  await pickRow(screen, platform, /^High Elf, SRD 5\.2\.1/).doubleTap();
  await pace(1800);

  await tour.caption('Build > Class: double-tap Fighter');
  await buildTab(screen, platform, 'CLASS').tap();
  await pace(1000);
  await pickRow(screen, platform, /^Fighter, SRD 5\.2\.1/).doubleTap();
  await pace(1800);

  await tour.caption('Build > Background: double-tap Acolyte');
  await buildTab(screen, platform, 'BACKGROUND').tap();
  await pace(1000);
  await pickRow(screen, platform, /^Acolyte, SRD 5\.2\.1/).doubleTap();
  await pace(1800);

  await tour.caption('Play: the live character sheet');
  await navLink(screen, platform, /^Play$/).tap();
  await expect(screen.getByLabel(/^Explain Proficiency, \+\d$/)).toBeVisible(SLOW);
  await pace(1800);

  await tour.caption('Explain Proficiency: how the engine got this number');
  await screen.getByLabel(/^Explain Proficiency, \+\d$/).tap();
  await expect(screen.getByText('Level 1')).toBeVisible(SLOW);
  await pace(2200);
  await tour.caption('Close the explanation');
  await screen.getByRole('button', 'CLOSE').tap();
  await expect(screen.getByText('Level 1')).toBeHidden(SLOW);
  await pace(1200);
});
