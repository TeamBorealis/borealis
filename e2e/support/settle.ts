import type { Box, LocatorLike } from './types.js';

const SETTLE_SAMPLES = 3;
const SETTLE_INTERVAL_MS = 60;
const SETTLE_MAX_MS = 3000;

/** Bounding box once it has stopped moving (flyouts slide in; pointer input should wait for it). */
export async function settledBox(locator: LocatorLike): Promise<Box> {
  const deadline = Date.now() + SETTLE_MAX_MS;
  let prev: Box | null = null;
  let stable = 0;
  for (;;) {
    const b = await locator.boundingBox();
    if (!b) throw new Error('settledBox: element has no box');
    stable = prev && prev.x === b.x && prev.y === b.y && prev.width === b.width && prev.height === b.height ? stable + 1 : 0;
    if (stable >= SETTLE_SAMPLES) return b;
    if (Date.now() > deadline) throw new Error('settledBox: box still moving after 3s');
    prev = b;
    await new Promise((r) => setTimeout(r, SETTLE_INTERVAL_MS));
  }
}
