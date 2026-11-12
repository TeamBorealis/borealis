import { isTauri } from "../lib/tauri";
import { useSettings } from "./SettingsContext";

export interface Platform {
  /** This is an iOS/Android build. Comes from the backend, never from the viewport width. */
  mobile: boolean;
  /** This build has the AI CLI integration (desktop except the Mac App Store build). Comes from the backend; false until
   * settings are read, so a Mac App Store build never shows AI text (a file dropped at launch gets the neutral message).
   * Without it no AI setting or entry is shown at all. */
  aiAvailable: boolean;
  /** The optional AI assistant is on (only where `aiAvailable`, off by default). The single gate for every AI entry. */
  aiEnabled: boolean;
  /** This is a store build (iOS, Google Play, the Mac App Store): no update check or notice, no Releases link. Comes from the
   * backend; true until settings are read, so a store build never shows a download link. */
  storeBuild: boolean;
  /** Settings have been read, so `mobile` and `aiEnabled` are real (redirects wait for it). */
  ready: boolean;
  /** The homebrew creator is shown (HC3; on by default from 1.5.0). The single gate for every creator entry; false until
   * settings are read, so a build with the setting off never flashes it. */
  creatorEnabled: boolean;
}

/**
 * HC3 (spec section 2): the creator shows when the `creatorEnabled` setting is on (on by default from 1.5.0, HC9; the web mock
 * turns it off with `?creator=0`), and always in a development build of the app (`tauri dev`). A plain `vite` page (the web mock, the
 * web e2e) is not a development build of the app: it follows the setting, so the switch-off state stays testable there.
 */
export function creatorOn(setting: boolean, devBuild: boolean = import.meta.env.DEV && isTauri()): boolean {
  return setting || devBuild;
}

/** One source of truth for "is mobile" and "is AI on". */
export function usePlatform(): Platform {
  const { settings, loaded } = useSettings();
  const mobile = settings.mobile;
  const aiAvailable = loaded && !mobile && settings.aiAvailable;
  return {
    mobile, aiAvailable, aiEnabled: aiAvailable && settings.ai.enabled, storeBuild: settings.storeBuild || !loaded, ready: loaded,
    creatorEnabled: loaded && creatorOn(settings.creatorEnabled),
  };
}
