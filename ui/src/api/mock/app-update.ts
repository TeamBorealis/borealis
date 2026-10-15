import type { AppUpdateStatus } from "../types";

/** Mock of the "new version available" check: tests set `status` and count `requests`. */
export class MockAppUpdate {
  status: AppUpdateStatus = {
    state: "done", current: "1.0.1", latest: "1.0.1", newer: false,
    url: "https://borealisbuilder.com/#download", releaseUrl: null, checkedAt: "2026-10-15T09:00:00Z",
  };
  /** How many times "Check now" asked for a fresh answer. */
  requests = 0;
  /** Times the download page was opened. */
  opened = 0;
}
