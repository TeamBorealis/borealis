import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, test } from 'e2e';
import type { TestFixtures } from 'e2e';

/**
 * Human-paced tour of the real macOS Borealis.app. The engine records the whole screen (screencapture); captions are only
 * LOGGED here (epoch ms) and burned in afterwards by scripts/collect-videos.mjs, which crops the video to the window
 * rectangle the engine saved next to it (video/window.json). Skipped unless E2E_TOUR=1.
 */
export type TourContext = TestFixtures;
export interface MacCaption { text: string; at: number }
export interface MacTourLog { name: string; platform: 'macos'; title: string; captions: MacCaption[]; endedAt: number }

const OUT_DIR = join(process.cwd(), '.e2e', 'tour');
const CAPTION_HOLD_MS = 1400;
export const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
export const pace = (ms = 1100): Promise<void> => sleep(ms);
export interface Tour { caption(text: string): Promise<void> }

export function macosTour(id: string, title: string, body: (ctx: TourContext, tour: Tour) => Promise<void>): void {
  describe(`tour-${id}-macos`, { tags: ['tour', 'macos', `tour-${id}`], platforms: ['macos'] }, () => {
    test(title, { video: process.env.E2E_TOUR ? 'on' : 'off', timeout: 600_000 }, async (ctx) => {
      test.skip(!process.env.E2E_TOUR, 'tour only');
      const log: MacTourLog = { name: `tour-${id}`, platform: 'macos', title, captions: [], endedAt: 0 };
      const tour: Tour = {
        async caption(text) { log.captions.push({ text, at: Date.now() }); await sleep(CAPTION_HOLD_MS); },
      };
      try {
        await body(ctx, tour);
        await pace(3000);
      } finally {
        log.endedAt = Date.now();
        mkdirSync(OUT_DIR, { recursive: true });
        writeFileSync(join(OUT_DIR, `macos-tour-${id}.json`), JSON.stringify(log, null, 2));
      }
    });
  });
}
