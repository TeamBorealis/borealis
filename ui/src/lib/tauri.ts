import { invoke } from "@tauri-apps/api/core";

/** Map of backend command name -> [args, result]. Extend as commands are added. */
export interface Commands {
  app_version: [Record<string, never>, string];
}

/** Typed wrapper around Tauri `invoke`. */
export function call<K extends keyof Commands>(
  cmd: K,
  ...args: Commands[K][0] extends Record<string, never> ? [] : [Commands[K][0]]
): Promise<Commands[K][1]> {
  return invoke<Commands[K][1]>(cmd, args[0]);
}

/** True when running inside the Tauri webview (false in plain browser/tests). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
