import { checkPermissions, invoke, requestPermissions } from "@tauri-apps/api/core";
import { isTauri } from "../../lib/tauri";

/** Outcome of one QR scan. Anything but `ok` falls back to the typed code (camera off, no camera, no plugin). */
export type ScanResult =
  | { kind: "ok"; text: string }
  | { kind: "cancelled" }
  | { kind: "denied" }
  | { kind: "unavailable"; reason: string };

/** The mode the scanned window must have (`m=` in the QR): a receiver scans a sending window. */
export type ScanExpect = "send" | "receive";

export interface Scanner {
  scan(expect: ScanExpect): Promise<ScanResult>;
  cancel(): Promise<void>;
}

const PLUGIN = "barcode-scanner";
type PermissionState = "granted" | "denied" | "prompt" | "prompt-with-rationale";

/** Phones (Tauri mobile): tauri-plugin-barcode-scanner in windowed mode, so our own scan screen draws over the camera. */
const pluginScanner: Scanner = {
  async scan() {
    let state: PermissionState;
    try {
      state = (await checkPermissions<{ camera: PermissionState }>(PLUGIN)).camera;
      if (state === "prompt" || state === "prompt-with-rationale") state = (await requestPermissions<{ camera: PermissionState }>(PLUGIN)).camera;
    } catch (e) {
      return { kind: "unavailable", reason: e instanceof Error ? e.message : String(e) };
    }
    if (state !== "granted") return { kind: "denied" };
    try {
      const scanned = await invoke<{ content: string }>(`plugin:${PLUGIN}|scan`, { windowed: true, formats: ["QR_CODE"] });
      return scanned?.content ? { kind: "ok", text: scanned.content } : { kind: "cancelled" };
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      return /cancel/i.test(reason) ? { kind: "cancelled" } : { kind: "unavailable", reason };
    }
  },
  async cancel() {
    try { await invoke(`plugin:${PLUGIN}|cancel`); } catch { /* nothing is scanning */ }
  },
};

/** Browser / tests: no camera. `?mockScan=1` pretends to read the other device's QR (web e2e). */
const mockScanner: Scanner = {
  async scan(expect) {
    if (new URLSearchParams(window.location.search).get("mockScan") !== "1") return { kind: "unavailable", reason: "No camera in this build." };
    await new Promise((r) => setTimeout(r, 400));
    return { kind: "ok", text: `borealis-link:v1?h=192.168.1.31:40112&fp=q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA&c=AAECAwQFBgcICQoLDA0ODw&n=Borealis%20on%20Mac&m=${expect}` };
  },
  async cancel() { /* nothing to stop */ },
};

let override: Scanner | null = null;
/** Tests replace the scanner. */
export function setScannerForTests(s: Scanner | null): void { override = s; }

/** The scanner for this build (the plugin only exists in the phone builds). */
export function scanner(mobile: boolean): Scanner {
  if (override) return override;
  return isTauri() && mobile ? pluginScanner : mockScanner;
}

/** True on iPhone / iPad (the Local Network prompt is iOS only). */
export function isIos(): boolean {
  if (typeof navigator === "undefined") return false;
  return /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
}
