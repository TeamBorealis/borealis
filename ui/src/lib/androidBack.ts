import { closeTopOverlay, openOverlayCount, subscribeOverlays } from "../components/controls/overlayStack";

/** Tauri's `onBackButtonPress` (from `@tauri-apps/api/app`, Android only). */
export type BackRegister = (handler: (payload: { canGoBack: boolean }) => void) => Promise<{ unregister: () => Promise<void> }>;

/**
 * Android back (mobile-facelift-spec sec. 5.10): while a sheet, dialog or menu is open, back closes the top one and stays
 * on the page. Tauri's AppPlugin hands the press to JS only while a listener is registered; otherwise it keeps its own
 * behaviour (the WebView goes back, then the app goes to the background). So the listener is registered exactly while an
 * overlay is open, and no native code is needed.
 */
export function installAndroidBack(register: BackRegister): () => void {
  let listener: Promise<{ unregister: () => Promise<void> }> | null = null;
  const sync = () => {
    const open = openOverlayCount() > 0;
    if (open && !listener) {
      listener = register(() => { closeTopOverlay(); });
      listener.catch(() => { listener = null; });
    } else if (!open && listener) {
      const done = listener;
      listener = null;
      void done.then((l) => l.unregister()).catch(() => undefined);
    }
  };
  const off = subscribeOverlays(sync);
  sync();
  return () => {
    off();
    if (listener) void listener.then((l) => l.unregister()).catch(() => undefined);
    listener = null;
  };
}
