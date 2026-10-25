import { invoke } from "@tauri-apps/api/core";
import { COMMANDS } from "../api/contract";
import { phoneOs } from "./phone";
import { isTauri } from "./tauri";

/**
 * B-48 O13: light haptics on choose, level up, rest and delete, on iOS and Android builds only (mobile-facelift-spec
 * sec. 5.13). The official Tauri haptics plugin (`tauri-plugin-haptics`) does the work; its JS binding
 * (`@tauri-apps/plugin-haptics`) is a thin `invoke` wrapper, called here directly as `pages/transfer/scanner.ts` does
 * for the barcode plugin. Desktop builds, plain browsers and the web mock have no plugin: nothing happens there.
 *
 * System setting: iOS's feedback generators already follow Settings > Sounds & Haptics > System Haptics. Android's
 * plugin vibrates through `Vibrator`, which ignores Settings > Touch feedback, so the backend's `haptics_enabled`
 * command reads that setting first and nothing fires when it is off.
 */
export type HapticEvent = "choose" | "levelUp" | "rest" | "delete";
export type ImpactStyle = "light";

export interface HapticsBackend {
  /** A selection tick (picker choice). */
  selection(): Promise<void>;
  /** A light impact (level up, rest, delete). */
  impact(style: ImpactStyle): Promise<void>;
  /** The platform's system haptics setting. */
  enabled(): Promise<boolean>;
}

const PLUGIN = "haptics";

/** The Tauri plugin on a phone build. */
const pluginBackend: HapticsBackend = {
  selection: () => invoke<void>(`plugin:${PLUGIN}|selection_feedback`),
  impact: (style) => invoke<void>(`plugin:${PLUGIN}|impact_feedback`, { style }),
  enabled: () => invoke<boolean>(COMMANDS.hapticsEnabled),
};

let override: HapticsBackend | null = null;

/** Tests replace the backend (null restores the platform's). */
export function setHapticsForTests(backend: HapticsBackend | null): void {
  override = backend;
}

/** The backend for this build, or null where there is no haptics plugin (desktop, browser, web mock). */
function backend(): HapticsBackend | null {
  if (override) return override;
  return isTauri() ? pluginBackend : null;
}

/** Fires the light haptic for `event`. Fire and forget: never throws, never blocks the action it accompanies. */
export function haptic(event: HapticEvent): void {
  if (!phoneOs()) return;
  const b = backend();
  if (!b) return;
  void (async () => {
    try {
      if (!(await b.enabled())) return;
      if (event === "choose") await b.selection();
      else await b.impact("light");
    } catch {
      // No haptic engine (simulator, old device) or the plugin is missing: the action itself already happened.
    }
  })();
}
