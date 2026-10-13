import { useEffect, useRef, useState, type ReactNode } from "react";
import type { StartupBridge } from "../../lib/startup";
import { SplashView } from "./SplashView";
import { useStartup } from "./useStartup";

const FADE_MS = 350;
/** The overlay is visible at least this long once shown, so a fast start does not flash it. */
const MIN_VISIBLE_MS = 1200;

interface Props {
  bridge: StartupBridge;
  version: string;
  children: ReactNode;
  /** Called once the app is mounted (backend commands are callable). */
  onReady?: () => void;
}

/**
 * Holds the app back until the backend finished loading (its commands do not exist before that).
 * On single-window platforms the splash fills the screen meanwhile and fades into the app; on
 * desktop this window is still hidden, so the gate is silent and the backend shows it when ready.
 */
export function StartupGate({ bridge, version, children, onReady }: Props) {
  const { progress, done } = useStartup(bridge, "main-loaded");
  const [phase, setPhase] = useState<"shown" | "leaving" | "gone">("shown");
  const shownAt = useRef(Date.now());
  const overlay = progress?.mobile ?? false;

  useEffect(() => {
    if (!done) return;
    onReady?.();
    // A timer, not requestAnimationFrame: the desktop window is still hidden and does not run frames.
    const t = window.setTimeout(() => bridge.signal("main-ready"), 60);
    return () => window.clearTimeout(t);
  }, [done, bridge, onReady]);

  useEffect(() => {
    if (!done || !overlay) return;
    const wait = Math.max(0, shownAt.current + MIN_VISIBLE_MS - Date.now());
    const t1 = window.setTimeout(() => setPhase("leaving"), wait);
    const t2 = window.setTimeout(() => setPhase("gone"), wait + FADE_MS);
    return () => { window.clearTimeout(t1); window.clearTimeout(t2); };
  }, [done, overlay]);

  if (!done) return <SplashView progress={progress} version={version} fullscreen />;
  return (
    <>
      {children}
      {overlay && phase !== "gone" && <SplashView progress={progress} version={version} fullscreen leaving={phase === "leaving"} />}
    </>
  );
}
