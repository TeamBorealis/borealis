import { useSyncExternalStore } from "react";
import type { Api, ImportPreview, PreviewImportRequest } from "../api/contract";
import { errorMessage } from "./useAsync";

/**
 * The rules-based content import flow (no AI): files, a folder or an OS-opened file are converted to a preview, shown in one
 * dialog (hosted by GlobalImport), then installed on confirmation. A tiny store so "+", drops, the Additional Content page and
 * "open with" all open the same dialog.
 */
interface State { preview: ImportPreview | null; busy: boolean }
let state: State = { preview: null, busy: false };
const listeners = new Set<() => void>();
const set = (next: State) => { state = next; listeners.forEach((l) => l()); };

export const useImportPreviewState = (): State => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state);

/** Convert `request` and open the preview. Resolves to the preview, or null after reporting the error through `onError`. */
export async function startImportPreview(api: Api, request: PreviewImportRequest, onError: (message: string) => void): Promise<ImportPreview | null> {
  set({ preview: null, busy: true });
  try {
    const preview = await api.previewImportPack({ request });
    set({ preview, busy: false });
    return preview;
  } catch (e) {
    set({ preview: null, busy: false });
    onError(errorMessage(e));
    return null;
  }
}

/** Close the dialog and drop the staged conversion. */
export function dismissImportPreview(api: Api): void {
  const p = state.preview;
  set({ preview: null, busy: false });
  if (p) void api.cancelImportPreview({ previewId: p.previewId }).catch(() => undefined);
}

export function setImportBusy(busy: boolean): void { set({ ...state, busy }); }
export function closeImportPreview(): void { set({ preview: null, busy: false }); }
