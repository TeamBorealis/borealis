import { expect } from 'e2e';
import { CHARACTER_NAME, createCharacter, expectBuildPage, freshInstall, mobileSuite, SLOW } from '../../support/mobile.js';

// m1: clean install -> Characters page -> New character -> Build.
// Real Rust backend; app data persists on the simulator, so each test starts from a fresh install.
mobileSuite('m1', 'creates a character and lands in Build', async ({ app, device, screen }) => {
  await freshInstall(device);
  await app.open();
  await expect(screen.getByText('WELCOME TO BOREALIS')).toBeVisible(SLOW);
  // Phone builds have no AI at all: nothing about an assistant, setup or pairing on the first screen.
  await expect(screen.getByText(/\bAI\b|assistant|codex|claude|pair/i)).toBeHidden();
  await createCharacter(screen, device, CHARACTER_NAME);
  await expect(screen.getByText('NEW CHARACTER, web dialogue', { exact: false })).toBeHidden(SLOW);
  // Build page: species step first, pending-choices prompt in the status bar.
  await expectBuildPage(screen);
  await expect(screen.getByText(/CHOOSE ONE FROM THE 'RACE' SELECTION/)).toBeVisible(SLOW);
});
