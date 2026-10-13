import { createViaShortcut, desktopSuite, expect, NAME, pressKey, SLOW } from './helpers.js';

// d2: Cmd+S opens the Save flyout ('Filename', 'Group'), Save character; quit + relaunch; the
// character is still listed (real Rust persistence).
desktopSuite('d2', 'Cmd+S saves; the character survives quit and relaunch', async (ctx) => {
  const { app, screen } = ctx;
  await app.open();
  await createViaShortcut(ctx, NAME);
  await pressKey(ctx, 'ControlOrMeta+s');
  await expect(screen.getByText(/^save character$/i).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/^filename$/i).first()).toBeVisible(SLOW);
  await expect(screen.getByText(/^group$/i).first()).toBeVisible(SLOW);
  await screen.getByRole('button', /^save character$/i).tap();
  await expect(screen.getByText(/^filename$/i).first()).toBeHidden(SLOW);
  await expect(screen.getByText(new RegExp(`'${NAME}' SAVED`, 'i'))).toBeVisible(SLOW);
  await app.restart();
  await expect(screen.getByText(/welcome to borealis/i).first()).toBeVisible(SLOW);
  await expect(screen.getByText(new RegExp(NAME)).first()).toBeVisible(SLOW);
});
