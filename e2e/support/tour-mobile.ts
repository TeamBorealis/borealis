import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, test } from '@e2e-dev/mobile';
import type { Device } from '@e2e-dev/mobile';
import type { TestFixtures } from 'e2e';
import type { MobilePlatform } from './mobile.js';

/**
 * Human-paced tour recordings of the real app on a simulator or emulator. A device webview takes no
 * injected overlay, so each caption is only LOGGED here (epoch ms) and burned into the MP4 afterwards by
 * scripts/collect-videos.mjs, which aligns it with the video through the report's artifact `startedAt`.
 */
type Context = TestFixtures & { device: Device };
export interface MobileCaption { text: string; at: number }
export interface MobileTourLog { name: string; platform: MobilePlatform; title: string; captions: MobileCaption[]; endedAt: number }

const OUT_DIR = join(process.cwd(), '.e2e', 'tour');
const CAPTION_HOLD_MS = 1400;
export const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Pause so a viewer can see what just changed. */
export const pace = (ms = 1100): Promise<void> => sleep(ms);

export interface Tour {
  /** Log a caption starting now and hold long enough to read it. */
  caption(text: string): Promise<void>;
}

/** Register a tour on iOS and on Android (tags `tour` + platform); skipped unless E2E_TOUR is set. */
export function mobileTour(id: string, title: string, body: (ctx: Context, platform: MobilePlatform, tour: Tour) => Promise<void>): void {
  for (const platform of ['ios', 'android'] as const) {
    describe(`tour-${id}-${platform}`, { tags: ['tour', platform, `tour-${id}`], platforms: [platform], requires: ['native-app'] }, () => {
      test(title, { video: process.env.E2E_TOUR ? 'on' : 'off', timeout: 600_000 }, async (context) => {
        test.skip(!process.env.E2E_TOUR, 'tour only');
        const log: MobileTourLog = { name: `tour-${id}`, platform, title, captions: [], endedAt: 0 };
        const tour: Tour = {
          async caption(text) {
            log.captions.push({ text, at: Date.now() });
            await sleep(CAPTION_HOLD_MS);
          },
        };
        try {
          await body(context, platform, tour);
          await pace(3500);
        } finally {
          log.endedAt = Date.now();
          mkdirSync(OUT_DIR, { recursive: true });
          writeFileSync(join(OUT_DIR, `${platform}-tour-${id}.json`), JSON.stringify(log, null, 2));
        }
      });
    });
  }
}
