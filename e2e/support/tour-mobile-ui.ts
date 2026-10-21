import type { TestFixtures } from 'e2e';
import type { MobilePlatform } from './mobile.js';

type Screen = TestFixtures['screen'];

/**
 * A Build picker row (one node named "<Name>, <Source>"). iOS reports it as a `switch` (the row carries
 * aria-pressed); the Android engine maps it to a labelled node, so it is matched by its label there.
 */
export function pickRow(screen: Screen, platform: MobilePlatform, name: RegExp) {
  return platform === 'ios' ? screen.getByRole('switch', name) : screen.getByLabel(name);
}

/** A Build section tab (SPECIES, CLASS, BACKGROUND...): a `button` on iOS, a `tab` on Android. */
export function buildTab(screen: Screen, platform: MobilePlatform, name: string) {
  return screen.getByRole(platform === 'ios' ? 'button' : 'tab', name);
}

/** Double-tap a Start-list character until its name shows in the title bar (a double tap sometimes lands as two single taps). */
export async function openCharacter(screen: Screen, entry: ReturnType<Screen['getByRole']>, name: string): Promise<void> {
  const { expect } = await import('e2e');
  for (let attempt = 0; ; attempt += 1) {
    await entry.doubleTap();
    try {
      await expect(screen.getByText(name.toUpperCase())).toBeVisible({ timeout: 12_000 });
      return;
    } catch (e) {
      if (attempt >= 2) throw e;
    }
  }
}
