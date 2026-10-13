import { isTauri } from "../lib/tauri";
import type { Api } from "./contract";
import { MockApi } from "./mock";
import { createTauriApi } from "./tauri";

export * from "./contract";
export { MockApi } from "./mock";
export { createTauriApi } from "./tauri";

/** Real backend inside Tauri, in-memory mock otherwise (or when VITE_USE_MOCK=1). */
export function createApi(): Api {
  const forceMock = import.meta.env.VITE_USE_MOCK === "1";
  if (isTauri() && !forceMock) return createTauriApi();
  // `?mock=mobile` simulates a phone build (no AI at all); used by the web e2e suites.
  const mobile = new URLSearchParams(window.location.search).get("mock") === "mobile";
  return new MockApi({ aiChunkDelayMs: 30, mobile });
}
