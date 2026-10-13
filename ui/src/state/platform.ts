import { useSettings } from "./SettingsContext";

export interface Platform {
  /** This is an iOS/Android build. Comes from the backend, never from the viewport width. */
  mobile: boolean;
  /** The optional AI assistant is on (desktop only, off by default). The single gate for every AI entry. */
  aiEnabled: boolean;
  /** Settings have been read, so `mobile` and `aiEnabled` are real (redirects wait for it). */
  ready: boolean;
}

/** One source of truth for "is mobile" and "is AI on". */
export function usePlatform(): Platform {
  const { settings, loaded } = useSettings();
  const mobile = settings.mobile;
  return { mobile, aiEnabled: !mobile && settings.ai.enabled, ready: loaded };
}
