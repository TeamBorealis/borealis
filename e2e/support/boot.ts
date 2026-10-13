import { expect } from 'e2e';

/** Route hash the UI starts on. The shell uses HashRouter: `/#/characters`. */
export const HOME = '/#/characters';

interface BootCtx {
  app: { open(path?: string): Promise<void> };
  screen: { getByRole(role: 'menuitem', name: string): unknown };
}

/**
 * Boot to a known state: fresh browser context (MockApi is in-memory, so every page load
 * reseeds it), the Characters start screen, shell menu visible.
 * Deterministic: no model involved.
 */
export async function bootToHome({ app, screen }: BootCtx, path: string = HOME): Promise<void> {
  await app.open(path);
  await expect(screen.getByRole('menuitem', 'File') as never).toBeVisible();
}
