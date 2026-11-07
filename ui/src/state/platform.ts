import { useSettings } from "./SettingsContext";

export interface Platform {
  /** This is an iOS/Android build. Comes from the backend, never from the viewport width. */
  mobile: boolean;
  /** This build has the AI CLI integration (desktop except the Mac App Store build). Comes from the backend. Without it no AI
   * setting or entry is shown at all. */
  aiAvailable: boolean;
  /** The optional AI assistant is on (only where `aiAvailable`, off by default). The single gate for every AI entry. */
  aiEnabled: boolean;
  /** This is a store build (iOS, Google Play, the Mac App Store): no update check or notice, no Releases link. Comes from the
   * backend; true until settings are read, so a store build never shows a download link. */
  storeBuild: boolean;
  /** Settings have been read, so `mobile` and `aiEnabled` are real (redirects wait for it). */
  ready: boolean;
}

/** One source of truth for "is mobile" and "is AI on". */
export function usePlatform(): Platform {
  const { settings, loaded } = useSettings();
  const mobile = settings.mobile;
  const aiAvailable = !mobile && settings.aiAvailable;
  return { mobile, aiAvailable, aiEnabled: aiAvailable && settings.ai.enabled, storeBuild: settings.storeBuild || !loaded, ready: loaded };
}
