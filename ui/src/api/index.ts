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
  // `?mockPeer=<ms>` makes a simulated other device answer Send/Receive windows (web e2e of the device transfer).
  const peer = Number(new URLSearchParams(window.location.search).get("mockPeer"));
  // `?mockBundle=<file name>` is the file File > Open bundle... "picks" (`tampered.borealis` and `notabundle.borealis` are refused).
  const bundle = new URLSearchParams(window.location.search).get("mockBundle");
  return new MockApi({ aiChunkDelayMs: 30, mobile, linkPeerAfterMs: Number.isFinite(peer) && peer > 0 ? peer : null, bundleFile: bundle });
}
