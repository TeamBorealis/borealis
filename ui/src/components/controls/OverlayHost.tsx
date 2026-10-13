import { createContext, type ReactNode, useContext } from "react";
import { createPortal } from "react-dom";

/**
 * Element that modal overlays (Flyout, Dialog) render into: the window root (.aur-window), outside the
 * scrolling main.aur-content. Android WebView (Chromium 133) never adds a position:fixed subtree that is
 * inserted inside a scroll container to its accessibility tree once that tree has been read, so a page
 * flyout rendered in place was invisible to TalkBack and UiAutomator. Null (no shell) renders in place.
 */
export const OverlayHostContext = createContext<HTMLElement | null>(null);

/** Renders a modal overlay into the window root when one is provided, else in place. */
export function OverlayPortal({ children }: { children: ReactNode }) {
  const host = useContext(OverlayHostContext);
  return host ? createPortal(children, host) : <>{children}</>;
}
