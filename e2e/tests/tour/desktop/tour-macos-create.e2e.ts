import { expect, NAME, pressKey, SLOW } from '../../desktop/helpers.js';
import { macosTour, pace } from '../../../support/tour-macos.js';

// Tour: the real macOS app (fresh data): create a character, build it, save it, preview the sheet. Demo only (E2E_TOUR=1).
macosTour('macos-create', 'Create, build, save and preview a character on macOS', async (ctx, tour) => {
  const { app, screen } = ctx;
  await app.open();
  await expect(screen.getByText(/welcome to borealis/i).first()).toBeVisible(SLOW);
  await tour.caption('Fresh profile: no setup wizard, nothing to configure');
  await pace(1200);

  await tour.caption('Cmd+N opens the New character flyout');
  await pressKey(ctx, 'ControlOrMeta+n');
  await expect(screen.getByRole('button', 'Create character')).toBeVisible(SLOW);
  await pace(1500);
  await tour.caption('Type a name');
  await screen.getByRole('textbox', /^name$/i).fill(NAME);
  await pace(1200);
  await tour.caption('Create character');
  await screen.getByRole('button', 'Create character').tap();
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
  await pace(1500);

  await tour.caption('Build > Species: double-click Elf');
  await screen.getByRole('checkbox', /^Elf, SRD 5\.2\.1/).doubleTap();
  await pace(1800);
  await tour.caption('Elven lineage: double-click High Elf');
  await screen.getByRole('checkbox', /^High Elf, SRD 5\.2\.1/).doubleTap();
  await pace(1800);
  await tour.caption('Build > Class: Fighter');
  await screen.getByText(/^Class$/).first().tap();
  await pace(1000);
  await screen.getByRole('checkbox', /^Fighter, SRD 5\.2\.1/).doubleTap();
  await pace(1800);
  await tour.caption('Build > Background: Acolyte');
  await screen.getByText(/^Background$/).first().tap();
  await pace(1000);
  await screen.getByRole('checkbox', /^Acolyte, SRD 5\.2\.1/).doubleTap();
  await pace(1800);

  await tour.caption('Cmd+S: save the character');
  await pressKey(ctx, 'ControlOrMeta+s');
  await expect(screen.getByText(/^filename$/i).first()).toBeVisible(SLOW);
  await pace(1800);
  await screen.getByRole('button', /^save character$/i).tap();
  await expect(screen.getByText(/^filename$/i).first()).toBeHidden(SLOW);
  await pace(1500);

  await tour.caption('Character Sheet: generate the preview');
  await screen.getByRole('link', /^character sheet$/i).tap();
  await screen.getByRole('button', 'Generate Preview').tap();
  await expect(screen.getByText(/No preview yet/)).toBeHidden(SLOW);
  await pace(3000);

  await tour.caption('Play: the live character sheet');
  await screen.getByRole('link', /^play$/i).tap();
  await pace(3000);
});
