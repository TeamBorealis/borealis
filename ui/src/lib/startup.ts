import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Payload of the backend's `startup-progress` event (src-tauri/src/startup.rs). */
export interface StartupProgress {
  message: string;
  percent: number;
  failed: boolean;
  /** Single-window platform: the UI shows the splash as an overlay. */
  mobile: boolean;
  details?: string;
}

/** Signals a webview sends so the backend can replay events it emitted before the listeners attached. */
export type StartupSignal = "splash-ready" | "main-loaded" | "main-ready";

/** Startup events from the backend (injectable for tests). */
export interface StartupBridge {
  onProgress(handler: (p: StartupProgress) => void): Promise<() => void>;
  onDone(handler: () => void): Promise<() => void>;
  signal(signal: StartupSignal): void;
  /** Close this window (the splash's error state: quits the app). */
  quit?(): void;
}

export const tauriStartupBridge: StartupBridge = {
  onProgress: (handler) => listen<StartupProgress>("startup-progress", (e) => handler(e.payload)),
  onDone: (handler) => listen("startup-done", () => handler()),
  signal: (signal) => { void emit(signal); },
  quit: () => { void getCurrentWindow().close(); },
};
