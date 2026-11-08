import { useEffect, useRef } from "react";
import type { Api } from "../../api/contract";

/**
 * Phones (B-221): call `onBackground` when the app leaves the foreground and `onForeground` when it comes back. Both the
 * backend's `app:lifecycle` event (the OS's own) and the page's `visibilitychange` report it; whichever comes first wins and
 * the other is ignored, so each change is handled once.
 */
export function useAppLifecycle(api: Api, enabled: boolean, onForeground: () => void, onBackground: () => void): void {
  const handlers = useRef({ onForeground, onBackground });
  handlers.current = { onForeground, onBackground };
  useEffect(() => {
    if (!enabled) return;
    // Starts in the foreground: Notes is on screen.
    let foregroundNow = true;
    const report = (foreground: boolean) => {
      if (foreground === foregroundNow) return;
      foregroundNow = foreground;
      if (foreground) handlers.current.onForeground(); else handlers.current.onBackground();
    };
    const off = api.subscribe("app:lifecycle", (e) => report(e.foreground));
    const onVisibility = () => report(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVisibility);
    return () => { off(); document.removeEventListener("visibilitychange", onVisibility); };
  }, [api, enabled]);
}
