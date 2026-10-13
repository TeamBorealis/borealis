import { createViaShortcut, desktopSuite, NAME } from './helpers.js';

// d1: Cmd+N opens the New character flyout; Create lands on Build asking for the species.
desktopSuite('d1', 'Cmd+N creates a character and lands on Build', async (ctx) => {
  await ctx.app.open();
  await createViaShortcut(ctx, NAME);
});
