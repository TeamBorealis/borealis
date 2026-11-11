import { useState } from "react";
import type { ContentPack, HomebrewExportResult } from "../../api/contract";
import { Alert, Button, Check, Modal, useToast } from "../../components";
import { downloadBase64 } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { errorMessage, useAsync } from "../../state/useAsync";

const ZIP_MIME = "application/zip";

/** The sentence about copied text (spec 14.2, HC8, CS1): it never names the books. */
export function copiedTextLine(n: number): string {
  return `${n} ${n === 1 ? "element contains" : "elements contain"} text copied from your imported books. That text isn't yours to share.`;
}

type ShareNavigator = Navigator & { canShare?: (d: { files: File[] }) => boolean; share?: (d: { files: File[] }) => Promise<void> };

/**
 * Export pack (spec 14.2): the pack as a `.borealis-pack.zip`. When elements hold text copied from the user's imported
 * books, the dialog says so and "Leave that text out" (default on) exports those descriptions empty. Desktop saves through
 * the backend's save dialog; Android opens the share sheet from the backend; elsewhere the bytes come back and a second
 * tap shares them (the share sheet needs a fresh gesture) or, on a desktop browser, they download.
 */
export function ExportPackDialog({ pack, onClose }: { pack: ContentPack; onClose: () => void }) {
  const api = useApi();
  const toast = useToast();
  const { mobile } = usePlatform();
  const rows = useAsync(() => api.homebrewListElements({ packId: pack.id }), [api, pack.id]);
  const copied = (rows.data ?? []).filter((r) => r.copiedText).length;
  const [leaveOut, setLeaveOut] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState<HomebrewExportResult | null>(null);

  const finish = (message: string) => { toast.push(message); onClose(); };
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.homebrewExportPack({ packId: pack.id, leaveOutCopiedText: leaveOut });
      if (r.path) { finish(`Exported "${pack.name}" to ${r.path}.`); return; }
      if (r.shared) { finish(`Shared ${r.fileName}.`); return; }
      setBusy(false);
      if (!r.dataBase64) return; // the save dialog was cancelled
      if (mobile) { setReady(r); return; }
      downloadBase64(r.fileName, ZIP_MIME, r.dataBase64);
      finish(`Saved ${r.fileName}.`);
    } catch (e) {
      setBusy(false);
      setError(errorMessage(e));
    }
  };
  const share = async (r: HomebrewExportResult) => {
    const file = new File([Uint8Array.from(atob(r.dataBase64 ?? ""), (c) => c.charCodeAt(0))], r.fileName, { type: ZIP_MIME });
    const nav = navigator as ShareNavigator;
    try {
      if (typeof nav.share === "function" && nav.canShare?.({ files: [file] })) {
        // Only the file: WebKit on iOS shares a title as a second, text item, so "Save to Files" saved a text.txt beside
        // the pack (WB-25, simulator; the same as B-148's sheet share).
        await nav.share({ files: [file] });
        finish(`Shared ${r.fileName}.`);
        return;
      }
      downloadBase64(r.fileName, ZIP_MIME, r.dataBase64 ?? "");
      finish(`Saved ${r.fileName}.`);
    } catch (e) {
      if ((e as { name?: string }).name !== "AbortError") setError(errorMessage(e));
    }
  };

  const action = mobile ? "Share" : "Export";
  return (
    <Modal title="Export pack" onClose={onClose} className="hb-export"
      footer={<>
        <Button onClick={onClose}>Cancel</Button>
        {ready
          ? <Button variant="primary" onClick={() => void share(ready)}>Share</Button>
          : <Button variant="primary" disabled={busy || rows.loading} onClick={() => void run()}>{action}</Button>}
      </>}>
      <div className="stack">
        <p>{ready ? `${ready.fileName} is ready to share.` : `"${pack.name}" goes into one pack file that anyone with Borealis can import. Unsaved changes stay on this device.`}</p>
        {!ready && copied > 0 && (
          <div role="group" aria-label="Copied text">
            <p>{copiedTextLine(copied)}</p>
            <Check label="Leave that text out" description="Those descriptions are exported empty." checked={leaveOut} onChange={(e) => setLeaveOut(e.target.checked)} />
          </div>
        )}
        {error && <Alert tone="danger">{error}</Alert>}
      </div>
    </Modal>
  );
}
