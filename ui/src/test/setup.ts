// Vitest setup for every UI test file (vite.config.ts `test.setupFiles`).
//
// B-197: under heavy machine load (load average 40+, e.g. a cargo build next to the suite) the full-app mock renders
// take seconds, so Testing Library's default 1 s wait for findBy* / waitFor ran out in tests that pass alone. A
// longer wait changes nothing for a passing test (it returns as soon as the element appears) and a real failure still
// fails, only later. Short waits that probe for something that must NOT appear keep their own explicit timeout.
// No retries: a test that fails, fails once.
import { configure } from "@testing-library/react";

/** How long findBy* and waitFor wait before failing (Testing Library's default is 1000 ms). */
export const ASYNC_UTIL_TIMEOUT_MS = 10_000;

configure({ asyncUtilTimeout: ASYNC_UTIL_TIMEOUT_MS });
