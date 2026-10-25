import { closeTopOverlay, openOverlayCount, subscribeOverlays } from "../components/controls/overlayStack";

/** Tauri's `onBackButtonPress` (from `@tauri-apps/api/app`, Android only). */
export type BackRegister = (handler: (payload: { canGoBack: boolean }) => void) => Promise<{ unregister: () => Promise<void> }>;

/** The app plugin's registration, loaded on first use (Android builds only call it). */
export const tauriBackButton: BackRegister = (handler) => import("@tauri-apps/api/app").then((m) => m.onBackButtonPress(handler));

/**
 * Android back (mobile-facelift-spec sec. 5.10, HW1 A2): while a sheet, dialog or menu is open, back closes the top one and
 * stays on the page; that is also the only way to close the full-screen phone Compendium. Tauri's AppPlugin hands the press
 * to JS only while a listener is registered; otherwise it keeps its own behaviour (the WebView goes back, then the app goes
 * to the background). So the listener is registered exactly while an overlay is open, and no native code is needed.
 */
export function installAndroidBack(register: BackRegister): () => void {
  let listener: Promise<{ unregister: () => Promise<void> }> | null = null;
  // Bumped whenever the listener goes away: a listener whose removal is still pending must not close a second sheet.
  let generation = 0;
  const stop = () => {
    generation += 1;
    const done = listener;
    listener = null;
    void done?.then((l) => l.unregister()).catch(() => undefined);
  };
  const sync = () => {
    const open = openOverlayCount() > 0;
    if (open && !listener) {
      const id = ++generation;
      const pending = register(() => { if (id === generation) closeTopOverlay(); });
      listener = pending;
      // No listener (older runtime, permission missing): Back keeps the system default.
      pending.catch(() => { if (listener === pending) listener = null; });
    } else if (!open && listener) {
      stop();
    }
  };
  const off = subscribeOverlays(sync);
  sync();
  return () => {
    off();
    stop();
  };
}
