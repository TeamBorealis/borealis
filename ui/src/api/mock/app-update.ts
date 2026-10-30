import type { AppUpdateStatus } from "../types";
import { EXTERNAL_LINKS } from "../../lib/links";
import { apiError } from "./errors";

/** Mock of the "new version available" check: tests set `status` and count `requests`. */
export class MockAppUpdate {
  status: AppUpdateStatus = {
    state: "done", current: "1.0.1", latest: "1.0.1", newer: false,
    url: "https://borealisbuilder.com/#download", releaseUrl: null, notes: null, checkedAt: "2026-10-15T09:00:00Z",
  };
  /** How many times "Check now" asked for a fresh answer. */
  requests = 0;
  /** Times the download page was opened. */
  opened = 0;
}

/** Mock of `open_link` (U15): an address that is not exactly one of the app's pages is refused like the backend does;
 * an allowed one is recorded, and mirrored on `window.__mockOpenedLinks` so the web e2e can read it. */
export function recordOpenedLink(opened: string[], url: string): void {
  if (!EXTERNAL_LINKS.includes(url)) throw apiError("invalid_argument", "Borealis only opens its own web pages.");
  opened.push(url);
  const w = globalThis as { __mockOpenedLinks?: string[] };
  w.__mockOpenedLinks = [...(w.__mockOpenedLinks ?? []), url];
}
